//! Aave v3 totals, read next to v4 so the two can be compared (docs/data-pipeline.md, "Aave v3
//! totals"). One task per chain: for every hour with no reading yet it finds a block at that hour
//! and reads every reserve of every market in two multicalls. The answers go to `raw.v3_calls`
//! as returned; `core.v3_reserve_state` decodes them in SQL.

use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aave_watch_protocol::codec::{Codec, selector};
use aave_watch_protocol::registry::V3Market;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::fetch::{ClickHouse, Rpc, env_u64, header};

/// 2026-03-24 00:00 UTC, the day of the first Aave v4 block: v3 history starts with v4's.
const LAUNCH: u64 = 1_774_310_400;
/// Hourly readings for this long back; one a day (00:00 UTC) before that.
const HOURLY: u64 = 7 * 86_400;
// shortcut: an hour is read once it is 30 minutes old, at a block inside its first 20 minutes,
// without waiting for each chain's settle tag; use the tag if a reorg that deep ever shows up.
const AGE: u64 = 1_800;
const WINDOW: u64 = 1_200;

pub(crate) async fn run_chain(markets: Vec<V3Market>, rpc: Rpc, ch: ClickHouse) -> Result<()> {
    let codec = Codec::new();
    let mut seen = BTreeMap::new();
    loop {
        if let Err(e) = pass(&markets, &rpc, &ch, &codec, &mut seen).await {
            eprintln!("{} v3: {e:#}; next try in a minute", markets[0].chain);
        }
        sleep(Duration::from_secs(60)).await;
    }
}

/// Reads every hour that is due and not marked done, newest first.
async fn pass(markets: &[V3Market], rpc: &Rpc, ch: &ClickHouse, codec: &Codec, seen: &mut BTreeMap<u64, u64>) -> Result<()> {
    let chain_id = markets[0].chain_id;
    let sql = format!("SELECT DISTINCT toUnixTimestamp(block_time) FROM raw.v3_calls WHERE chain_id = {chain_id} AND market = '' FORMAT TSV");
    let done: HashSet<u64> = ch.query(&sql, String::new()).await?.lines().filter_map(|l| l.parse().ok()).collect();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    for hour in targets(now).into_iter().filter(|h| !done.contains(h)) {
        let mut rows = Vec::new();
        let block = block_at(rpc, seen, hour).await?;
        if let Some(block) = block {
            for market in markets {
                rows.extend(read_market(codec, rpc, market, block).await.with_context(|| format!("{} at block {block}", market.market))?);
            }
        }
        // The marker: this hour is done for the chain, whether or not a market answered.
        rows.push(json!({"chain_id": chain_id, "market": "", "block_number": block.unwrap_or(0), "block_time": hour,
                         "target": "", "selector": "", "args": "", "ok": 1, "result": ""}));
        ch.insert("raw.v3_calls", &rows).await?;
    }
    Ok(())
}

/// The hours to hold a reading for, newest first: every hour of the last week, then every day.
fn targets(now: u64) -> Vec<u64> {
    let newest = (now - AGE) / 3600 * 3600;
    let hourly = newest.saturating_sub(HOURLY);
    let hours = (hourly..=newest).rev().step_by(3600);
    let days = (LAUNCH..hourly).rev().filter(|t| t % 86_400 == 0);
    hours.chain(days).filter(|t| *t >= LAUNCH).collect()
}

/// A block whose time is in `[time, time + WINDOW]`, or the first after it. `None` when the chain
/// had no block yet. `seen` (time -> number) keeps every header fetched, so later searches start
/// close. Interpolates between the nearest known blocks on each side.
async fn block_at(rpc: &Rpc, seen: &mut BTreeMap<u64, u64>, time: u64) -> Result<Option<u64>> {
    async fn fetch(rpc: &Rpc, seen: &mut BTreeMap<u64, u64>, tag: String) -> Result<(u64, u64)> {
        let h = header(&rpc.call("eth_getBlockByNumber", json!([tag, false])).await?)?;
        seen.insert(h.time, h.number);
        Ok((h.time, h.number))
    }
    let mut above = match seen.range(time..).next() {
        Some((t, n)) => (*t, *n),
        None => fetch(rpc, seen, "latest".into()).await?,
    };
    ensure!(above.0 >= time, "the head is older than the hour asked for");
    let mut below = match seen.range(..time).next_back() {
        Some((t, n)) => (*t, *n),
        None => {
            let mut step = 100_000;
            loop {
                let number = above.1.saturating_sub(step).max(1);
                let found = fetch(rpc, seen, format!("0x{number:x}")).await?;
                if found.0 < time {
                    break found;
                }
                if number == 1 {
                    return Ok(None);
                }
                above = found;
                step *= 4;
            }
        }
    };
    loop {
        if above.0 <= time + WINDOW || above.1 - below.1 <= 1 {
            return Ok(Some(above.1));
        }
        let guess = guess(below, above, time + WINDOW / 2);
        let found = fetch(rpc, seen, format!("0x{guess:x}")).await?;
        if found.0 < time { below = found } else { above = found }
    }
}

/// Linear guess of the block at `time` between two known (time, number) points, strictly inside.
fn guess(below: (u64, u64), above: (u64, u64), time: u64) -> u64 {
    let blocks = (above.1 - below.1) as u128 * (time - below.0) as u128 / (above.0 - below.0).max(1) as u128;
    (below.1 + blocks as u64).clamp(below.1 + 1, above.1 - 1)
}

/// One market at one block as `raw.v3_calls` rows. Empty when the market did not exist at that
/// block or the node no longer has its state.
async fn read_market(codec: &Codec, rpc: &Rpc, market: &V3Market, block: u64) -> Result<Vec<Value>> {
    let provider = market.provider.to_lowercase();
    let first = vec![
        (provider.clone(), selector("getPriceOracle()")),
        (provider, selector("getPoolDataProvider()")),
        (market.pool.to_lowercase(), selector("getReservesList()")),
    ];
    let (time, answers) = match rpc.multicall(codec, block, &first).await {
        Ok(a) => a,
        Err(e) if gone(&format!("{e:#}")) => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    // a call to an address with no code yet succeeds with no data
    if answers.iter().any(|(ok, data)| !ok || data.len() < 66) {
        return Ok(vec![]);
    }
    let (oracle, data) = (format!("0x{}", &answers[0].1[26..66]), format!("0x{}", &answers[1].1[26..66]));
    let mut calls = vec![(oracle.clone(), selector("BASE_CURRENCY_UNIT()"))];
    for asset in addresses(&answers[2].1) {
        let word = format!("{:0>64}", &asset[2..]);
        calls.push((data.clone(), format!("{}{word}", selector("getATokenTotalSupply(address)"))));
        calls.push((data.clone(), format!("{}{word}", selector("getTotalDebt(address)"))));
        calls.push((oracle.clone(), format!("{}{word}", selector("getAssetPrice(address)"))));
        calls.push((asset, selector("decimals()")));
    }
    let mut rows = Vec::with_capacity(first.len() + calls.len());
    let mut push = |calls: &[(String, String)], answers: Vec<(bool, String)>| {
        for ((target, data), (ok, result)) in calls.iter().zip(answers) {
            rows.push(json!({"chain_id": market.chain_id, "market": market.market, "block_number": block, "block_time": time,
                             "target": target, "selector": &data[..10], "args": format!("0x{}", &data[10..]), "ok": ok as u8, "result": result}));
        }
    };
    push(&first, answers);
    for chunk in calls.chunks(env_u64("FETCH_MULTICALL", 200) as usize) {
        push(chunk, rpc.multicall(codec, block, chunk).await?.1);
    }
    Ok(rows)
}

/// The node says it no longer has this block's state: asking again will not help.
fn gone(error: &str) -> bool {
    let e = error.to_lowercase();
    ["pruned", "historic", "missing trie node", "not available", "header not found", "state not found"].iter().any(|s| e.contains(s))
}

/// An ABI-encoded `address[]` (0x hex) as lowercase addresses.
fn addresses(data: &str) -> Vec<String> {
    let words: Vec<&str> = data.as_bytes()[2..].chunks(64).filter_map(|w| std::str::from_utf8(w).ok()).collect();
    let count = words.get(1).and_then(|w| usize::from_str_radix(&w[w.len().saturating_sub(8)..], 16).ok()).unwrap_or(0);
    words.iter().skip(2).take(count).filter(|w| w.len() == 64).map(|w| format!("0x{}", w[24..].to_lowercase())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_hourly_for_a_week_then_daily_back_to_launch() {
        let now = LAUNCH + 30 * 86_400 + 3 * 3600 + 1_000; // 03:16, so 02:00 is the newest hour old enough
        let t = targets(now);
        assert_eq!(t[0], LAUNCH + 30 * 86_400 + 2 * 3600);
        assert_eq!(t[1], t[0] - 3600);
        assert_eq!(*t.last().unwrap(), LAUNCH);
        assert_eq!(t.len(), 7 * 24 + 1 + 24); // 169 hours, then days 0..=23
        assert!(t.windows(2).all(|w| w[0] > w[1]));
        assert!(targets(LAUNCH + 4000).iter().all(|t| *t >= LAUNCH));
    }

    #[test]
    fn the_guess_lands_between_the_known_blocks() {
        assert_eq!(guess((1000, 100), (2000, 200), 1500), 150);
        assert_eq!(guess((1000, 100), (2000, 200), 1000), 101);
        assert_eq!(guess((1000, 100), (1000, 102), 1000), 101);
    }

    #[test]
    fn an_address_array_decodes() {
        let data = format!("0x{:064x}{:064x}{:0>64}{:0>64}", 32, 2, "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "dac17f958d2ee523a2206206994597c13d831ec7");
        assert_eq!(addresses(&data), ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "0xdac17f958d2ee523a2206206994597c13d831ec7"]);
        assert!(addresses("0x").is_empty());
    }
}
