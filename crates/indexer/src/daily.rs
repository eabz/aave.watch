//! `indexer daily`: account history for the days before the tip read every account on a schedule
//! (docs/data-pipeline.md, "Wallet history"). For each chain and each 00:00 UTC it reads
//! `getUserAccountData` for every account open on that day, at one block, stores the answers in
//! `raw.calls` (`reason = daily`) and their decoded rows in `core.user_account_state`. Run once;
//! a day that is done is marked in `meta.cursor` and skipped by the next run.

use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::registry;
use anyhow::{Context, Result, ensure};
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::fetch::{ClickHouse, Rpc, env_u64};
use crate::model::{Entity, World};
use crate::process::{Rows, State, process_block};
use crate::v3::{block_at, gone};

const DAY: u64 = 86_400;
/// A day is read once its midnight is this old: its block has settled on every chain.
const AGE: u64 = 3_600;

/// `daily [chain_id [from to]]`, dates as `YYYY-MM-DD`, both inclusive.
pub async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let usage = "usage: aave-watch-indexer daily [chain_id [from to]], dates as YYYY-MM-DD";
    let only: Option<u32> = args.first().map(|a| a.parse()).transpose().ok().context(usage)?;
    let date = |i: usize, default: &str| args.get(i).map_or(default, String::as_str).to_string();
    let (from, to) = (date(1, "1970-01-02"), date(2, "2100-01-01"));
    // the dates go into SQL text
    ensure!([&from, &to].iter().all(|d| d.len() == 10 && d.bytes().all(|b| b.is_ascii_digit() || b == b'-')), usage);
    let ch = ClickHouse::new();
    let mut tasks = tokio::task::JoinSet::new();
    for chain in registry::chains().into_iter().filter(|c| only.is_none_or(|id| id == c.chain_id)) {
        let var = format!("RPC_HTTP_{}", chain.chain_id);
        match std::env::var(&var) {
            Ok(url) => drop(tasks.spawn(run_chain(chain, Rpc::new(url), ch.clone(), from.clone(), to.clone()))),
            Err(_) => eprintln!("{}: {var} is not set, chain skipped", chain.name),
        }
    }
    while let Some(ended) = tasks.join_next().await {
        ended??;
    }
    Ok(())
}

async fn run_chain(chain: registry::Chain, rpc: Rpc, ch: ClickHouse, from: String, to: String) -> Result<()> {
    let (id, codec) = (chain.chain_id, Codec::new());
    // Decoding needs each Spoke's oracle and that oracle's decimals; both never change.
    let mut world = ch.world(&chain, &codec, u64::MAX).await?;
    let mut state = State::load(&ch, id).await?;
    let unix = |date: &str| format!("toUnixTimestamp(toDateTime(toDate('{date}'), 'UTC'))");
    let sql = format!(
        "SELECT toUnixTimestamp(min(block_time)), toUnixTimestamp(minIf(block_time, reason = 'tier')), {}, {} \
         FROM core.user_account_state WHERE chain_id = {id} FORMAT TSV",
        unix(&from), unix(&to)
    );
    let bounds: Vec<u64> = ch.query(&sql, String::new()).await?.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    ensure!(bounds.len() == 4, "{}: no bounds", chain.name);
    let sql = format!("SELECT DISTINCT stream FROM meta.cursor WHERE chain_id = {id} AND stream LIKE 'daily:%' FORMAT TSV");
    let done: HashSet<u64> = ch.query(&sql, String::new()).await?.lines().filter_map(|l| l.strip_prefix("daily:")?.parse().ok()).collect();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let todo: Vec<u64> = days(bounds[0], bounds[1], now, bounds[2], bounds[3]).into_iter().filter(|d| !done.contains(d)).collect();
    eprintln!("{}: {} days to read", chain.name, todo.len());
    let mut seen = BTreeMap::new();
    for day in todo {
        let (block, accounts) = loop {
            match read_day(id, &rpc, &ch, &codec, &mut world, &mut state, &mut seen, day).await {
                Ok(read) => break read,
                Err(e) => eprintln!("{}: {}: {e:#}; next try in a minute", chain.name, &crate::live::utc(day)[..10]),
            }
            sleep(Duration::from_secs(60)).await;
        };
        eprintln!("{}: {} block {block}, {accounts} accounts", chain.name, &crate::live::utc(day)[..10]);
    }
    Ok(())
}

/// The midnights to hold a reading for: after the chain's first reading, up to the day the tier
/// readings began (from then on every account is read at least hourly), inside `[from, to]`.
/// `tier` is 0 when the chain has no tier reading yet.
fn days(first: u64, tier: u64, now: u64, from: u64, to: u64) -> Vec<u64> {
    if first == 0 {
        return Vec::new();
    }
    let dense = if tier > 0 { tier / DAY * DAY } else { u64::MAX };
    let end = dense.min(now.saturating_sub(AGE) / DAY * DAY).min(to);
    let start = ((first / DAY + 1) * DAY).max(from.div_ceil(DAY) * DAY);
    (start..=end).step_by(DAY as usize).collect()
}

/// Reads one day and marks it done: `(block read, accounts read)`. `(0, 0)` when nobody had a
/// position, the chain had no block, or the node no longer has that block's state.
#[allow(clippy::too_many_arguments)]
async fn read_day(
    id: u32,
    rpc: &Rpc,
    ch: &ClickHouse,
    codec: &Codec,
    world: &mut World,
    state: &mut State,
    seen: &mut BTreeMap<u64, u64>,
    day: u64,
) -> Result<(u64, usize)> {
    // Open on that day: the account's last reading before it shows collateral or debt.
    let sql = format!(
        "SELECT spoke, user FROM core.user_account_state WHERE chain_id = {id} AND block_time < toDateTime({day}, 'UTC') \
         GROUP BY spoke, user HAVING argMax(total_collateral_value_raw > 0 OR total_debt_value_ray > 0, block_number) FORMAT TSV"
    );
    let text = ch.query(&sql, String::new()).await?;
    let calls: Vec<(String, String)> =
        text.lines().filter_map(|l| l.split_once('\t')).flat_map(|(spoke, user)| Entity::User(spoke.into(), user.into()).calls(codec)).collect();
    let block = if calls.is_empty() { None } else { block_at(rpc, seen, day).await? };
    let mut read = (0, 0);
    if let Some(block) = block {
        let chunks: Vec<&[(String, String)]> = calls.chunks(env_u64("FETCH_MULTICALL", 200) as usize).collect();
        let answers: Vec<_> =
            stream::iter(0..chunks.len()).map(|i| rpc.multicall(codec, block, chunks[i])).buffered(env_u64("FETCH_PARALLEL", 4) as usize).collect().await;
        let (mut raw, mut ok, mut pruned) = (Vec::new(), Vec::new(), false);
        for (chunk, answer) in chunks.iter().zip(answers) {
            let (time, results) = match answer {
                Ok(answer) => answer,
                Err(e) if gone(&format!("{e:#}")) => {
                    pruned = true;
                    break;
                }
                Err(e) => return Err(e),
            };
            for ((target, data), (success, result)) in chunk.iter().zip(results) {
                let (selector, args) = (&data[..10], format!("0x{}", &data[10..]));
                raw.push(json!({"chain_id": id, "block_number": block, "block_time": time, "target": target, "selector": selector,
                                "args": args, "ok": success as u8, "result": result, "reason": "daily"}));
                if success {
                    ok.push(json!({"t": time, "target": target, "selector": selector, "args": args, "result": result, "reason": "daily"}));
                }
            }
        }
        if !pruned {
            // Decoded here, not by the processor: its cursor is long past these blocks. A reparse
            // finds the same answers in raw.calls and writes the same rows.
            let mut out = Rows::default();
            process_block(codec, world, state, id, block, &[], &ok.iter().collect::<Vec<&Value>>(), &mut out);
            let rows = out.0.remove("core.user_account_state").unwrap_or_default();
            ch.insert("raw.calls", &raw).await?;
            ch.insert("core.user_account_state", &rows).await?;
            read = (block, rows.len());
        }
    }
    ch.insert("meta.cursor", &[json!({"chain_id": id, "stream": format!("daily:{day}"), "block_number": read.0, "block_hash": ""})]).await?;
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_run_from_the_first_reading_to_the_first_tier_day() {
        let d = |n: u64| n * DAY;
        // first reading on day 10 at 05:00, tier readings from day 13 at 16:00, now day 20
        let all = days(d(10) + 18_000, d(13) + 57_600, d(20), 0, u64::MAX);
        assert_eq!(all, [d(11), d(12), d(13)]);
        // no tier reading yet: up to the last midnight that is an hour old
        assert_eq!(days(d(10), 0, d(12) + 600, 0, u64::MAX), [d(11)]);
        assert_eq!(days(d(10), 0, d(12) + AGE, 0, u64::MAX), [d(11), d(12)]);
        // a chosen range, both ends inclusive; a chain with no reading has no days
        assert_eq!(days(d(10), d(13), d(20), d(12), d(12)), [d(12)]);
        assert!(days(d(10), d(13), d(20), d(14), d(19)).is_empty());
        assert!(days(0, 0, d(20), 0, u64::MAX).is_empty());
    }
}
