//! The fetcher's view calls. Spec: docs/data-pipeline.md, "Scheduler".
//!
//! One task per chain walks from the `calls` cursor to the `raw` cursor. It reads the logs the
//! fetcher already stored (no node), plans which views each block needs, reads them through one
//! multicall per block and stores every answer in `raw.calls`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::registry;
use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::fetch::{ClickHouse, Rpc, env_u64};
use crate::model::{World, decode, text};


/// (target, calldata) -> why it is read. `event` wins over `heartbeat` when both ask.
pub(crate) type Plan = BTreeMap<(String, String), Reason>;

/// Why a view is read; stored in `raw.calls.reason`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Reason {
    Event,
    Heartbeat,
    Meta,
    /// A health factor read that was due by its tier; live only.
    Tier,
}

pub async fn run_chain(chain: registry::Chain, rpc: Rpc, ch: ClickHouse, held_below: Arc<AtomicU64>) -> Result<()> {
    let (id, codec) = (chain.chain_id, Codec::new());
    let mut cursor = ch.cursor(id, "calls").await?.unwrap_or(chain.start_block - 1);
    let mut world = World::new(&chain);
    for log in ch.logs(id, chain.start_block, cursor, true).await? {
        if let Some((_, name, args)) = decode(&codec, &log) {
            world.apply(text(&log, "address"), name, &args);
        }
    }
    let mut anchor = ch.last_call(id, cursor).await?;
    let idle = Duration::from_millis((chain.block_time_ms * 8).clamp(5_000, 120_000));
    // About ten hours of blocks per round: one query for logs, one insert of calls.
    let chunk = 36_000_000 / chain.block_time_ms;
    eprintln!(
        "{}: calls from block {}, {} assets, {} Spoke lines, {} reserves",
        chain.name, cursor + 1, world.assets.len(), world.hub_spokes.len(), world.reserves.len()
    );
    loop {
        let to = ch.cursor(id, "raw").await?.unwrap_or(0).min(cursor + chunk).min(held_below.load(Relaxed));
        if to <= cursor {
            sleep(idle).await;
            continue;
        }
        let logs = ch.logs(id, cursor + 1, to, false).await?;
        let plans = plan(&codec, &mut world, &mut anchor, &logs, to, chain.block_time_ms);
        let mut read = stream::iter(plans)
            .map(|(block, calls): (u64, Plan)| read_block(id, &rpc, &codec, block, calls))
            .buffered(env_u64("FETCH_PARALLEL", 4) as usize);
        let mut rows = Vec::new();
        while let Some(block) = read.next().await {
            rows.extend(block);
        }
        drop(read);
        ch.insert("raw.calls", &rows).await?;
        ch.insert("meta.cursor", &[json!({"chain_id": id, "stream": "calls", "block_number": to, "block_hash": ""})]).await?;
        if !rows.is_empty() {
            eprintln!("{}: {} calls stored, calls cursor at {to}", chain.name, rows.len());
        }
        cursor = to;
    }
}

/// Which views to read at which block, for logs in block order up to block `to`.
/// `anchor` is the last block read and its time; it carries the hourly heartbeat across rounds.
fn plan(
    codec: &Codec,
    world: &mut World,
    anchor: &mut Option<(u64, u64)>,
    logs: &[Value],
    to: u64,
    block_time_ms: u64,
) -> BTreeMap<u64, Plan> {
    let mut plans: BTreeMap<u64, Plan> = BTreeMap::new();
    let heartbeat = |world: &World, plan: &mut Plan| {
        let all = world.everything().into_iter().flat_map(|e| e.calls(codec));
        for call in all.chain(world.price_calls(codec)) {
            plan.entry(call).or_insert(Reason::Heartbeat);
        }
    };
    let mut logs = logs.iter().peekable();
    while let Some(log) = logs.next() {
        let (Some(block), Some(time)) = (log["block_number"].as_u64(), log["t"].as_u64()) else { continue };
        // Prices at every block read: an event is valued at the price of its own block. Planned
        // after the block's last log, so a reserve added in the block is priced too.
        let last_of_block = logs.peek().is_none_or(|next| next["block_number"].as_u64() != Some(block));
        let prices = |world: &World, plan: &mut Plan| {
            for call in world.price_calls(codec) {
                plan.entry(call).or_insert(Reason::Event);
            }
        };
        if anchor.is_none_or(|(b, _)| b != block) {
            // Hours with no event between the last read and this block still get their read.
            for (quiet, _) in hour_blocks(*anchor, Some((block, time)), to, block_time_ms) {
                heartbeat(world, plans.entry(quiet).or_default());
            }
            if anchor.is_none_or(|(_, t)| t / 3600 != time / 3600) {
                heartbeat(world, plans.entry(block).or_default());
            }
            *anchor = Some((block, time));
        }
        let plan = plans.entry(block).or_default();
        if let Some((_, name, args)) = decode(codec, log) {
            let contract = text(log, "address");
            for (target, view) in world.apply(contract, name, &args) {
                plan.insert((target, codec.call(view, &[])), Reason::Meta);
            }
            for call in world.touched(contract, name, &args).iter().flat_map(|e| e.calls(codec)) {
                plan.insert(call, Reason::Event);
            }
        }
        if last_of_block && !plan.is_empty() {
            prices(world, plan);
        }
    }
    for (quiet, time) in hour_blocks(*anchor, None, to, block_time_ms) {
        heartbeat(world, plans.entry(quiet).or_default());
        *anchor = Some((quiet, time));
    }
    plans.retain(|_, plan| !plan.is_empty());
    plans
}

/// The first block of each hour that starts after `anchor` (block, unix time) and has no event:
/// `(block, the hour's start)`. Between two known blocks the block is interpolated; past the last
/// one (`next` is `None`) it is estimated from the chain's block time, up to `limit`. The estimate
/// only chooses which block to read: the row carries the block's real time.
fn hour_blocks(anchor: Option<(u64, u64)>, next: Option<(u64, u64)>, limit: u64, block_time_ms: u64) -> Vec<(u64, u64)> {
    let Some((b1, t1)) = anchor else { return Vec::new() };
    // `end` is the first hour that needs no heartbeat here: the next event's own hour.
    let (per_second, last, end) = match next {
        Some((b2, t2)) if b2 > b1 && t2 > t1 => ((b2 - b1) as f64 / (t2 - t1) as f64, b2 - 1, t2 / 3600 * 3600),
        Some(_) => return Vec::new(),
        None => (1000.0 / block_time_ms as f64, limit, u64::MAX),
    };
    let mut out: Vec<(u64, u64)> = Vec::new();
    let mut hour = (t1 / 3600 + 1) * 3600;
    while hour < end {
        let block = b1 + ((hour - t1) as f64 * per_second).ceil() as u64;
        if block > last {
            break;
        }
        if block > b1 && out.last().is_none_or(|(b, _)| *b != block) {
            out.push((block, hour));
        }
        hour += 3600;
    }
    out
}

/// Reads one block's plan and returns its `raw.calls` rows. Retries until the node answers:
/// skipping a block would leave a hole that only an archive node can fill later.
pub(crate) async fn read_block(chain_id: u32, rpc: &Rpc, codec: &Codec, block: u64, plan: Plan) -> Vec<Value> {
    let calls: Vec<((String, String), Reason)> = plan.into_iter().collect();
    let mut rows = Vec::with_capacity(calls.len());
    for chunk in calls.chunks(env_u64("FETCH_MULTICALL", 200) as usize) {
        let targets: Vec<(String, String)> = chunk.iter().map(|(call, _)| call.clone()).collect();
        let mut wait = Duration::from_secs(1);
        let (time, results) = loop {
            match rpc.multicall(codec, block, &targets).await {
                Ok(answer) => break answer,
                Err(e) => {
                    eprintln!("rpc: calls at block {block}: {e:#}; retrying in {wait:?}");
                    sleep(wait).await;
                    wait = (wait * 2).min(Duration::from_secs(60));
                }
            }
        };
        for (((target, data), reason), (ok, result)) in chunk.iter().zip(results) {
            rows.push(json!({
                "chain_id": chain_id, "block_number": block, "block_time": time,
                "target": target, "selector": &data[..10], "args": format!("0x{}", &data[10..]),
                "ok": ok as u8, "result": result, "reason": format!("{reason:?}").to_lowercase(),
            }));
        }
    }
    rows
}

impl Rpc {
    /// Every call at `block` in one `eth_call`: the block's time, then (ok, result) per call.
    /// Multicall2 is not deployed on every chain, so its code is supplied as a state override.
    async fn multicall(&self, codec: &Codec, block: u64, calls: &[(String, String)]) -> Result<(u64, Vec<(bool, String)>)> {
        use aave_watch_protocol::codec::{MULTICALL, MULTICALL_CODE};
        let params = json!([
            {"to": MULTICALL, "data": codec.multicall(calls)},
            format!("0x{block:x}"),
            {MULTICALL: {"code": MULTICALL_CODE.trim()}},
        ]);
        let reply = self.call("eth_call", params).await?;
        let (time, results) = codec.multicall_result(reply.as_str().unwrap_or("")).context("multicall reply does not decode")?;
        anyhow::ensure!(results.len() == calls.len(), "multicall answered {} of {} calls", results.len(), calls.len());
        Ok((time, results))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aave_watch_protocol::abi;

    #[test]
    fn quiet_hours_get_one_block_each() {
        // 12s blocks, anchor at 00:10:00, next event three hours later at 03:10:00.
        let (t, hour) = (1_800_000_000 / 3600 * 3600 + 600, 3600);
        let between = hour_blocks(Some((1000, t)), Some((1900, t + 3 * hour)), 5000, 12_000);
        // 01:00 and 02:00 are quiet; 03:00 belongs to the event's own hour
        assert_eq!(between, [(1250, t - 600 + hour), (1550, t - 600 + 2 * hour)]);
        // the next event in the same or the following hour needs no extra read
        assert!(hour_blocks(Some((1000, t)), Some((1010, t + 120)), 5000, 12_000).is_empty());
        assert!(hour_blocks(Some((1000, t)), Some((1300, t + hour)), 5000, 12_000).is_empty());
        // past the last event: estimated from the block time, never beyond the limit
        assert_eq!(hour_blocks(Some((1000, t)), None, 1600, 12_000), [(1250, t - 600 + hour), (1550, t - 600 + 2 * hour)]);
        assert!(hour_blocks(Some((1000, t)), None, 1100, 12_000).is_empty());
        assert!(hour_blocks(None, None, 1100, 12_000).is_empty());
        // no time passed, or a block out of order: nothing, and no division by zero
        assert!(hour_blocks(Some((1000, t)), Some((1000, t)), 5000, 12_000).is_empty());
    }

    #[test]
    fn a_block_with_an_event_reads_what_it_touched_plus_the_first_heartbeat() {
        let codec = Codec::new();
        let chain = registry::chains().into_iter().find(|c| c.chain_id == 1).unwrap();
        let hub = chain.hubs[0].address.to_lowercase();
        let mut world = World::new(&chain);
        let word = |n: u64| format!("0x{n:064x}");
        let log = |block: u64, t: u64, event: &str, topic2: &str, data: String| {
            json!({"block_number": block, "t": t, "address": hub, "topic0": abi::topic0("hub", event).unwrap(),
                   "topic1": word(3), "topic2": topic2, "topic3": "", "data": data})
        };
        let token = "0x000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
        let spoke = "0x00000000000000000000000094e7a5dcbe816e498b89ab752661904e2f56c485";
        let logs = [
            log(100, 1_800_000_000, "AddAsset", token, word(18)),
            log(100, 1_800_000_000, "AddSpoke", spoke, "0x".into()),
            log(101, 1_800_000_012, "Draw", spoke, format!("{}{}", word(5), &word(6)[2..])),
        ];
        let mut anchor = None;
        let plans = plan(&codec, &mut world, &mut anchor, &logs, 200, 12_000);
        assert_eq!(anchor, Some((101, 1_800_000_012)));
        assert_eq!(plans.keys().copied().collect::<Vec<_>>(), [100, 101]);
        // block 100: the token's symbol and name; its heartbeat ran before anything existed
        assert_eq!(plans[&100].values().filter(|r| **r == Reason::Meta).count(), 2);
        // block 101: the asset (4 views) and the Spoke line (3 views), read because of the event
        assert_eq!(plans[&101].values().filter(|r| **r == Reason::Event).count(), 7);
        assert!(plans[&101].values().all(|r| *r != Reason::Heartbeat));
    }
}
