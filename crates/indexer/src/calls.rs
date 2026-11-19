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


/// Contracts whose logs were stored behind the cursors (a Hub admitted late, a Spoke configured
/// before its Hub announced it): address -> (already done up to, logs stored up to).
pub(crate) type Late = BTreeMap<String, (u64, u64)>;

/// SQL condition for what a pass over late contracts reads: their own logs, and the registry
/// events of every contract, so `World` knows at each block what the chain had announced.
pub(crate) fn late_logs(late: &Late) -> String {
    let addresses: Vec<String> = late.keys().map(|a| format!("'{a}'")).collect();
    format!("address IN ({}) OR {}", addresses.join(","), crate::fetch::registry_events())
}

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
    let mut world = ch.world(&chain, &codec, cursor).await?;
    let mut anchor = ch.last_call(id, cursor, "").await?;
    let idle = Duration::from_millis((chain.block_time_ms * 8).clamp(5_000, 120_000));
    // About ten hours of blocks per round: one query for logs, one insert of calls.
    let chunk = 36_000_000 / chain.block_time_ms;
    eprintln!(
        "{}: calls from block {}, {} assets, {} Spoke lines, {} reserves",
        chain.name, cursor + 1, world.assets.len(), world.hub_spokes.len(), world.reserves.len()
    );
    loop {
        let to = ch.cursor(id, "raw").await?.unwrap_or(0).min(cursor + chunk).min(held_below.load(Relaxed));
        // After the cursor is read: the fetcher marks a late contract before it stores the range
        // that announced it, so the marks of everything up to `to` are in by now.
        if late(&chain, &rpc, &ch, &codec).await? {
            world = ch.world(&chain, &codec, cursor).await?;
        }
        if to <= cursor {
            sleep(idle).await;
            continue;
        }
        let logs = ch.logs(id, cursor + 1, to, "").await?;
        let plans = plan(&codec, &mut world, &mut anchor, &logs, to, chain.block_time_ms, None);
        let rows = read_blocks(id, &rpc, &codec, plans).await;
        ch.insert("raw.calls", &rows).await?;
        ch.insert("meta.cursor", &[json!({"chain_id": id, "stream": "calls", "block_number": to, "block_hash": ""})]).await?;
        if !rows.is_empty() {
            eprintln!("{}: {} calls stored, calls cursor at {to}", chain.name, rows.len());
        }
        cursor = to;
    }
}

/// The view calls of contracts that are late (see `Late`): the reads the walk above would have
/// made for them had they been watched from their first block, planned by the same `plan` and
/// stored in `raw.calls`. `Ok(false)` when nothing is late. Progress is marked in `meta.cursor`
/// (`late-calls:{address}`) after every batch, so a pass that is stopped goes on from there.
pub(crate) async fn late(chain: &registry::Chain, rpc: &Rpc, ch: &ClickHouse, codec: &Codec) -> Result<bool> {
    let id = chain.chain_id;
    let late = ch.pending(id, "late:", "late-calls:").await?;
    let Some(upto) = late.values().map(|(_, upto)| *upto).max() else { return Ok(false) };
    let mut world = World::new(chain);
    world.hubs.extend(ch.hubs(id).await?);
    let logs = ch.logs(id, chain.start_block, upto, &late_logs(&late)).await?;
    // A pass that goes on after an earlier one keeps the hourly reads in step with it.
    let (targets, done) = (late.keys().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(","), late.values().map(|(done, _)| *done).max().unwrap_or(0));
    let mut anchor = ch.last_call(id, done, &format!("target IN ({targets})")).await?;
    let mut plans = plan(codec, &mut world, &mut anchor, &logs, upto, chain.block_time_ms, Some(&late));
    eprintln!("{}: {} late contracts, calls to read at {} blocks up to {upto}", chain.name, late.len(), plans.len());
    let mark = |block: u64| -> Vec<Value> {
        let row = |(a, (_, upto)): (&String, &(u64, u64))| json!({"chain_id": id, "stream": format!("late-calls:{a}"), "block_number": block.min(*upto), "block_hash": ""});
        late.iter().map(row).collect()
    };
    while let Some((&first, _)) = plans.first_key_value() {
        let rest = plans.split_off(&plans.keys().nth(500).copied().unwrap_or(u64::MAX));
        let batch = std::mem::replace(&mut plans, rest);
        let last = batch.last_key_value().map_or(first, |(block, _)| *block);
        let rows = read_blocks(id, rpc, codec, batch).await;
        ch.insert("raw.calls", &rows).await?;
        ch.insert("meta.cursor", &mark(last)).await?;
        eprintln!("{}: {} late calls stored, up to block {last}", chain.name, rows.len());
    }
    ch.insert("meta.cursor", &mark(upto)).await?;
    Ok(true)
}

/// Reads every block's plan, a few blocks at a time, and returns the `raw.calls` rows in order.
async fn read_blocks(chain_id: u32, rpc: &Rpc, codec: &Codec, plans: BTreeMap<u64, Plan>) -> Vec<Value> {
    let mut read = stream::iter(plans)
        .map(|(block, calls): (u64, Plan)| read_block(chain_id, rpc, codec, block, calls))
        .buffered(env_u64("FETCH_PARALLEL", 4) as usize);
    let mut rows = Vec::new();
    while let Some(block) = read.next().await {
        rows.extend(block);
    }
    rows
}

/// Which views to read at which block, for logs in block order up to block `to`.
/// `anchor` is the last block read and its time; it carries the hourly heartbeat across rounds.
/// With `late`, only the late contracts are read, each for the blocks it still lacks; every
/// other log only tells `world` what exists.
fn plan(
    codec: &Codec,
    world: &mut World,
    anchor: &mut Option<(u64, u64)>,
    logs: &[Value],
    to: u64,
    block_time_ms: u64,
    late: Option<&Late>,
) -> BTreeMap<u64, Plan> {
    let mut plans: BTreeMap<u64, Plan> = BTreeMap::new();
    let mine = |target: &str| late.is_none_or(|late| late.contains_key(target));
    let heartbeat = |world: &World, plan: &mut Plan| {
        let all = world.everything().into_iter().flat_map(|e| e.calls(codec));
        for call in all.chain(world.price_calls(codec)).chain(world.source_calls(codec)).filter(|(target, _)| mine(target)) {
            plan.entry(call).or_insert(Reason::Heartbeat);
        }
    };
    // Prices at every block read: an event is valued at the price of its own block. Planned
    // after the block's last log, so a reserve added in the block is priced too.
    let prices = |world: &World, plan: Option<&mut Plan>| {
        let Some(plan) = plan.filter(|plan| !plan.is_empty()) else { return };
        for call in world.price_calls(codec).into_iter().filter(|(target, _)| mine(target)) {
            plan.entry(call).or_insert(Reason::Event);
        }
    };
    // The block whose logs are being planned, until a log of a later block closes it.
    let mut open: Option<u64> = None;
    for log in logs {
        let (Some(block), Some(time)) = (log["block_number"].as_u64(), log["t"].as_u64()) else { continue };
        if let Some(done) = open.filter(|b| *b != block) {
            prices(world, plans.get_mut(&done));
            open = None;
        }
        if late.is_some_and(|late| late.get(text(log, "address")).is_none_or(|(done, upto)| block <= *done || block > *upto)) {
            if let Some((_, name, args)) = decode(codec, log) {
                world.apply(text(log, "address"), name, &args);
            }
            continue;
        }
        open = Some(block);
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
    }
    prices(world, open.and_then(|block| plans.get_mut(&block)));
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
    pub(crate) async fn multicall(&self, codec: &Codec, block: u64, calls: &[(String, String)]) -> Result<(u64, Vec<(bool, String)>)> {
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
        let plans = plan(&codec, &mut world, &mut anchor, &logs, 200, 12_000, None);
        assert_eq!(anchor, Some((101, 1_800_000_012)));
        assert_eq!(plans.keys().copied().collect::<Vec<_>>(), [100, 101]);
        // block 100: the token's symbol and name; its heartbeat ran before anything existed
        assert_eq!(plans[&100].values().filter(|r| **r == Reason::Meta).count(), 2);
        // block 101: the asset (4 views) and the Spoke line (3 views), read because of the event
        assert_eq!(plans[&101].values().filter(|r| **r == Reason::Event).count(), 7);
        assert!(plans[&101].values().all(|r| *r != Reason::Heartbeat));

        // A late pass over the same logs, where a second Hub is the late contract: only it is
        // read, and only for the blocks it lacks; the first Hub's logs still build the registry.
        let late_hub = "0xd138fcb0e8c52fdbb2ac4573fa4d8925cf11e9c2";
        let mut logs = logs.to_vec();
        for (block, event, topic2, data) in [(100, "AddAsset", token, word(18)), (101, "AddSpoke", spoke, "0x".to_string()), (150, "Draw", spoke, format!("{}{}", word(5), &word(6)[2..]))] {
            let mut late_log = log(block, 1_800_000_000 + (block - 100) * 12, event, topic2, data);
            late_log["address"] = json!(late_hub);
            logs.push(late_log);
        }
        logs.sort_by_key(|l| l["block_number"].as_u64());
        let mut world = World::new(&chain);
        world.hubs.insert(late_hub.to_string());
        let late: Late = [(late_hub.to_string(), (100, 150))].into();
        let plans = plan(&codec, &mut world, &mut None, &logs, 150, 12_000, Some(&late));
        // block 100 is done already; 101 and 150 are read, on the late Hub alone
        assert_eq!(plans.keys().copied().collect::<Vec<_>>(), [101, 150]);
        assert!(plans.values().flat_map(|p| p.keys()).all(|(target, _)| target == late_hub));
        assert_eq!(plans[&150].values().filter(|r| **r == Reason::Event).count(), 7);
        // both Hubs are in the registry: the first Hub's asset and line, and the late Hub's
        assert_eq!((world.assets.len(), world.hub_spokes.len()), (2, 2));
        assert!(late_logs(&late).starts_with(&format!("address IN ('{late_hub}') OR topic0 IN (")));
    }
}
