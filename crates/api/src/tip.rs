//! The unsettled tip in Dragonfly: block heads and the live health factor ranking.

use aave_watch_protocol::keys;
use redis::AsyncCommands;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Longest a request waits on Dragonfly before treating it as down.
const WAIT: std::time::Duration = std::time::Duration::from_millis(750);

async fn bounded<T>(work: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::time::timeout(WAIT, work).await.unwrap_or_else(|_| Err("Dragonfly is unreachable: no answer in time".into()))
}

pub struct Tip {
    client: redis::Client,
    // One shared connection that reconnects by itself; opened on first use so the API starts
    // (and serves ClickHouse) while Dragonfly is down.
    shared: tokio::sync::OnceCell<redis::aio::ConnectionManager>,
}

impl Tip {
    pub fn new(client: redis::Client) -> Self {
        Self { client, shared: tokio::sync::OnceCell::new() }
    }

    pub async fn conn(&self) -> Result<redis::aio::ConnectionManager, String> {
        // Bounded: with Dragonfly down the client would retry for seconds, and every request
        // that prefers the tip would wait with it instead of falling back to ClickHouse.
        let open = self.shared.get_or_try_init(|| redis::aio::ConnectionManager::new(self.client.clone()));
        match tokio::time::timeout(WAIT, open).await {
            Ok(Ok(c)) => Ok(c.clone()),
            Ok(Err(e)) => Err(format!("Dragonfly is unreachable: {e}")),
            Err(_) => Err("Dragonfly is unreachable: no answer in time".into()),
        }
    }

    pub async fn ping(&self) -> Result<(), String> {
        let mut c = self.conn().await?;
        bounded(async { redis::cmd("PING").query_async::<String>(&mut c).await.map(|_| ()).map_err(|e| format!("Dragonfly is unreachable: {e}")) }).await
    }

    /// `head:{chain}` for each chain. Chains with no head yet are left out.
    pub async fn heads(&self, chains: &[u32]) -> Result<Vec<Value>, String> {
        bounded(self.heads_inner(chains)).await
    }

    async fn heads_inner(&self, chains: &[u32]) -> Result<Vec<Value>, String> {
        let mut c = self.conn().await?;
        let mut out = Vec::new();
        for &chain in chains {
            let h: HashMap<String, String> = c.hgetall(keys::head(chain)).await.map_err(|e| e.to_string())?;
            if !h.is_empty() {
                out.push(json!({ "chain_id": chain, "number": h.get("number"), "settled": h.get("settled"), "time": h.get("time") }));
            }
        }
        Ok(out)
    }

    /// Accounts with health factor below `max_hf` on the given `(chain, spoke, spoke_name)` list,
    /// lowest first. `None` when Dragonfly has no ranking for any of them (indexer not live yet).
    pub async fn positions(&self, spokes: &[(u32, String, String)], max_hf: f64, limit: usize) -> Result<Option<Vec<Value>>, String> {
        bounded(self.positions_inner(spokes, max_hf, limit)).await
    }

    async fn positions_inner(&self, spokes: &[(u32, String, String)], max_hf: f64, limit: usize) -> Result<Option<Vec<Value>>, String> {
        let mut c = self.conn().await?;
        let mut rows = Vec::new();
        let mut any = false;
        for (chain, spoke, name) in spokes {
            let key = keys::by_health(*chain, spoke);
            let n: u64 = c.zcard(&key).await.map_err(|e| e.to_string())?;
            if n == 0 { continue; }
            any = true;
            let users: Vec<(String, f64)> = c.zrangebyscore_limit_withscores(&key, 0f64, format!("({max_hf}"), 0, limit as isize)
                .await.map_err(|e| e.to_string())?;
            for (user, hf) in users {
                let h: HashMap<String, String> = c.hgetall(keys::user(*chain, spoke, &user)).await.map_err(|e| e.to_string())?;
                let num = |k: &str| h.get(k).and_then(|v| v.parse::<f64>().ok());
                rows.push(json!({
                    "chain_id": chain, "spoke": spoke, "spoke_name": name, "user": user,
                    "health_factor": hf,
                    "collateral_usd": num("collateral_usd"), "debt_usd": num("debt_usd"),
                    "risk_premium": num("risk_premium"),
                    "block_number": h.get("block"), "block_time": h.get("time"),
                }));
            }
        }
        if !any { return Ok(None); }
        rows.sort_by(|a, b| a["health_factor"].as_f64().partial_cmp(&b["health_factor"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
        rows.truncate(limit);
        Ok(Some(rows))
    }
}
