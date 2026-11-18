//! The fetcher's `live` state: the unsettled blocks above the raw cursor.
//! Spec: docs/data-pipeline.md, "From backfill to live".
//!
//! One task per chain. Every interval it reads the head, fetches the logs of the new blocks and
//! one multicall at the head (prices, what the logs touched, the users whose health factor is
//! due), and appends the batch as fetched to the Dragonfly stream `raw:{chain_id}`. The processor
//! reads that stream. ClickHouse is not written here: settled blocks reach it through the
//! catch-up walk.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::{keys, registry};
use anyhow::{Context, Result};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::calls::{Plan, Reason, read_block};
use crate::fetch::{ClickHouse, Rpc, env_u64, header, log_row};
use crate::model::{Entity, World, decode, text};

/// Polled heads remembered, to find where a reorg forked.
const REMEMBERED: usize = 256;

pub async fn run_chain(chain: registry::Chain, rpc: Rpc, ch: ClickHouse, held_below: Arc<AtomicU64>) -> Result<()> {
    let url = std::env::var("DRAGONFLY_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let mut db = ConnectionManager::new(redis::Client::open(url)?).await?;
    loop {
        // A session ends on any error (node, Dragonfly, a reorg deeper than we remember) and
        // starts again from the raw cursor: the same path as the first start.
        if let Err(e) = session(&chain, &rpc, &ch, &mut db, &held_below).await {
            eprintln!("{}: tip: {e:#}; starting again in 10s", chain.name);
        }
        // Not live: nothing is held, the calls walk is free.
        held_below.store(u64::MAX, Relaxed);
        sleep(Duration::from_secs(10)).await;
    }
}

async fn session(chain: &registry::Chain, rpc: &Rpc, ch: &ClickHouse, db: &mut ConnectionManager, held_below: &AtomicU64) -> Result<()> {
    let (id, codec) = (chain.chain_id, Codec::new());
    // shortcut: the head is polled, not pushed over a WebSocket, and state is read once per
    // poll at the head, not at every block; lower TIP_INTERVAL_MS, or move to eth_subscribe,
    // when a reading a few seconds old is too old.
    let interval = Duration::from_millis(env_u64("TIP_INTERVAL_MS", 3_000).max(chain.block_time_ms));
    let blocks_in = |ms: u64| (ms / chain.block_time_ms).max(1);

    // The tip starts from full settled state: wait for the processor to be near the raw cursor.
    let start = loop {
        let raw = ch.cursor(id, "raw").await?.unwrap_or(0);
        let core = ch.cursor(id, "core").await?.unwrap_or(0);
        if raw > 0 && raw.saturating_sub(core) <= blocks_in(900_000) {
            break raw;
        }
        sleep(Duration::from_secs(30)).await;
    };

    let mut world = World::new(chain);
    for log in ch.logs(id, chain.start_block, u64::MAX, true).await? {
        if let Some((_, name, args)) = decode(&codec, &log) {
            world.apply(text(&log, "address"), name, &args);
        }
    }
    // Every borrower is due at once; the per-poll cap below spreads the first reads.
    let sql = format!("SELECT spoke, user FROM core.user_account_latest FINAL WHERE chain_id = {id} AND debt_usd > 0 FORMAT TSV");
    let borrowers: Vec<(u64, String)> = ch.query(&sql, String::new()).await?.lines().filter_map(|l| l.split_once('\t')).map(|(s, u)| (0, keys::due_member(s, u))).collect();
    for batch in borrowers.chunks(5_000) {
        let _: () = redis::cmd("ZADD").arg(keys::due(id)).arg("NX").arg(batch).query_async(db).await?;
    }

    let first = header(&rpc.call("eth_getBlockByNumber", json!([format!("0x{start:x}"), false])).await?)?;
    let mut heads: VecDeque<(u64, String)> = [(first.number, first.hash)].into();
    let mut last_time = first.time;
    // Health factor reads by tier exist only here, so they are kept: held until their block is
    // settled, then written to raw.calls, where the processor finds them like any other call.
    let mut held: Vec<Value> = Vec::new();
    eprintln!("{}: tip from block {}, {} borrowers scheduled, every {interval:?}", chain.name, start + 1, borrowers.len());

    loop {
        sleep(interval).await;
        let head = header(&rpc.call("eth_getBlockByNumber", json!(["latest", false])).await?)?;
        let (last, last_hash) = heads.back().cloned().context("no head remembered")?;
        if head.number <= last {
            continue;
        }
        // Is the block we last published still on the chain?
        let mut from = last + 1;
        if head.number != last + 1 || head.parent_hash != last_hash {
            let hash_at = async |n: u64| anyhow::Ok(header(&rpc.call("eth_getBlockByNumber", json!([format!("0x{n:x}"), false])).await?)?.hash);
            if hash_at(last).await? != last_hash {
                while let Some((n, hash)) = heads.back().cloned() {
                    if hash_at(n).await? == hash {
                        break;
                    }
                    heads.pop_back();
                }
                from = heads.back().context("reorg deeper than the heads remembered")?.0 + 1;
                eprintln!("{}: reorg from block {from}", chain.name);
                held.retain(|row| row["block_number"].as_u64().is_some_and(|b| b < from));
                append(db, id, &json!({"v": keys::ENVELOPE_VERSION, "type": "reorg", "chain_id": id, "from_block": from})).await?;
            }
        }

        let watch: BTreeSet<&String> = world.hubs.iter().chain(world.hub_spokes.iter().map(|(_, _, s)| s)).chain(world.oracles.values()).collect();
        let watch: Vec<String> = watch.into_iter().cloned().collect();
        let logs = rpc.logs_once(&watch, from, head.number).await?;
        let (times, _) = rpc.block_times(&logs).await?;
        let mut rows = Vec::with_capacity(logs.len());
        let mut plan = Plan::new();
        for log in &logs {
            let number = log["blockNumber"].as_str().and_then(|n| u64::from_str_radix(n.trim_start_matches("0x"), 16).ok()).context("log without a block")?;
            let mut row = log_row(id, log, *times.get(&number).context("log without a block time")?)?;
            row["t"] = row["block_time"].clone();
            if let Some((_, name, args)) = decode(&codec, &row) {
                let contract = text(&row, "address");
                world.apply(contract, name, &args);
                for call in world.touched(contract, name, &args).iter().flat_map(|e| e.calls(&codec)) {
                    plan.insert(call, Reason::Event);
                }
            }
            rows.push(row);
        }
        for call in world.price_calls(&codec) {
            plan.entry(call).or_insert(Reason::Event);
        }
        if head.time / 3600 != last_time / 3600 {
            for call in world.everything().iter().flat_map(|e| e.calls(&codec)) {
                plan.entry(call).or_insert(Reason::Heartbeat);
            }
        }
        let due: Vec<String> = db.zrangebyscore_limit(keys::due(id), 0, head.number, 0, env_u64("TIP_MAX_USERS", 500) as isize).await?;
        for (spoke, user) in due.iter().filter_map(|member| member.split_once(':')) {
            for call in Entity::User(spoke.to_string(), user.to_string()).calls(&codec) {
                plan.entry(call).or_insert(Reason::Tier);
            }
        }

        let mut calls = read_block(id, rpc, &codec, head.number, plan).await;
        held.extend(calls.iter().filter(|c| c["reason"] == "tier").cloned());
        // The next read of each account read now, by health factor tier.
        for call in &mut calls {
            call["t"] = call["block_time"].clone();
            let Some(("getUserAccountData", inputs, account)) = codec.result(text(call, "selector"), text(call, "args"), text(call, "result")) else { continue };
            let member = keys::due_member(text(call, "target"), text(&inputs, "user"));
            let health = account["healthFactor"].as_str().and_then(|h| h.parse::<f64>().ok()).unwrap_or(f64::MAX) / 1e18;
            let _: () = match (text(&account, "totalDebtValueRay") == "0", health) {
                (true, _) => db.zrem(keys::due(id), member).await?,
                (_, h) if h < 1.1 => db.zadd(keys::due(id), member, head.number + 1).await?,
                (_, h) if h < 1.5 => db.zadd(keys::due(id), member, head.number + blocks_in(300_000)).await?,
                _ => db.zadd(keys::due(id), member, head.number + blocks_in(3_600_000)).await?,
            };
        }

        let settled = ch.cursor(id, "raw").await?.unwrap_or(0);
        let (now_settled, still_held) = held.into_iter().partition(|row| row["block_number"].as_u64().is_some_and(|b| b <= settled));
        held = still_held;
        ch.insert("raw.calls", &now_settled).await?;
        held_below.store(settled.min(head.number), Relaxed);
        let batch = json!({
            "v": keys::ENVELOPE_VERSION, "type": "blocks", "chain_id": id, "settled": settled,
            "head": {"number": head.number, "hash": head.hash, "parent_hash": head.parent_hash, "time": head.time},
            "logs": rows, "calls": calls,
        });
        append(db, id, &batch).await?;
        last_time = head.time;
        heads.push_back((head.number, head.hash));
        if heads.len() > REMEMBERED {
            heads.pop_front();
        }
    }
}

/// Appends one entry to `raw:{chain_id}`, dropping entries older than the retention.
async fn append(db: &mut ConnectionManager, chain_id: u32, entry: &Value) -> Result<()> {
    xadd(db, &keys::raw(chain_id), entry).await
}

/// `XADD key MINID ~ <now - retention> * json <entry>`: every stream keeps one hour.
pub(crate) async fn xadd(db: &mut ConnectionManager, stream: &str, entry: &Value) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let _: String = redis::cmd("XADD")
        .arg(stream).arg("MINID").arg("~").arg(now - keys::STREAM_RETENTION_MS)
        .arg("*").arg(keys::STREAM_FIELD).arg(entry.to_string())
        .query_async(db)
        .await?;
    Ok(())
}
