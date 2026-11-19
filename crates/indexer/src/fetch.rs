//! `indexer fetch`: the only process that talks to a node. Spec: docs/data-pipeline.md.
//!
//! This file is the `catching_up` state: it walks settled blocks over HTTP in parallel ranges,
//! stores every log of the watched contracts in `raw.logs`, and keeps walking as the settle tag
//! advances. The watch list grows by itself: new Hubs (hubs.rs) and what the Hubs announce.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::{abi, registry};

use crate::model::World;
use anyhow::{Context, Result, anyhow, bail};
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use tokio::time::sleep;

/// Ceiling for the blocks asked of one `eth_getLogs`; the working size adapts below it.
const MAX_RANGE: u64 = 10_000;
/// `ORACLE()` on a standard Spoke.
const ORACLE_SELECTOR: &str = "0x38013f02";

pub async fn run() -> Result<()> {
    let ch = ClickHouse::new();
    let mut tasks = tokio::task::JoinSet::new();
    let mut v4 = Vec::new();
    for chain in registry::chains() {
        let var = format!("RPC_HTTP_{}", chain.chain_id);
        match env::var(&var) {
            Ok(url) => {
                let rpc = Rpc::new(url);
                v4.push((chain.clone(), rpc.clone()));
                // The tip holds its health factor reads until their block settles; the calls
                // walk must not pass a block whose reads are still held. See tip.rs.
                let held_below = std::sync::Arc::new(AtomicU64::new(u64::MAX));
                tasks.spawn(crate::calls::run_chain(chain.clone(), rpc.clone(), ch.clone(), held_below.clone()));
                tasks.spawn(crate::tip::run_chain(chain.clone(), rpc.clone(), ch.clone(), held_below));
                tasks.spawn(run_chain(chain, rpc, ch.clone()));
            }
            Err(_) => eprintln!("{}: {var} is not set, chain skipped", chain.name),
        }
    }
    // Names from Aave's API, the Spoke rows of meta.spokes, and a second look at Hub candidates.
    tasks.spawn(crate::hubs::refresh(ch.clone(), v4));
    // Aave v3 totals: one task per chain that has a v3 market and an endpoint. See v3.rs.
    let mut v3: std::collections::BTreeMap<u32, Vec<registry::V3Market>> = Default::default();
    for market in registry::v3_markets() {
        v3.entry(market.chain_id).or_default().push(market);
    }
    for (chain_id, markets) in v3 {
        match env::var(format!("RPC_HTTP_{chain_id}")) {
            Ok(url) => drop(tasks.spawn(crate::v3::run_chain(markets, Rpc::new(url), ch.clone()))),
            Err(_) => eprintln!("{} v3: RPC_HTTP_{chain_id} is not set, chain skipped", markets[0].chain),
        }
    }
    // A chain task loops forever; if one ends, stop so systemd restarts the process.
    match tasks.join_next().await {
        Some(ended) => ended?,
        None => bail!("no RPC_HTTP_<chain_id> is set"),
    }
}

async fn run_chain(chain: registry::Chain, rpc: Rpc, ch: ClickHouse) -> Result<()> {
    let codec = Codec::new();
    let first = chain.start_block;
    let mut world = World::new(&chain);
    // Hubs announced while no walk was looking (an older binary, or blocks the one-off backfill
    // has not reached): admitted here, their logs fetched by `watch_list`.
    let upto = ch.cursor(chain.chain_id, "raw").await?.unwrap_or(0);
    let mut hubs = world.hubs.clone();
    hubs.extend(ch.hubs(chain.chain_id).await?);
    crate::hubs::scan(&chain, &rpc, &ch, &mut hubs, upto).await?;
    let mut watch = watch_list(&chain, &rpc, &ch, &codec, &mut world, upto, true).await?;
    let size = AtomicU64::new(env_u64("FETCH_RANGE", 2_000).clamp(1, MAX_RANGE));
    // How often to look at the settle tag once caught up.
    let idle = Duration::from_millis((chain.block_time_ms * 8).clamp(5_000, 120_000));
    eprintln!("{}: watching {} addresses", chain.name, watch.len());
    loop {
        match round(&chain, &rpc, &ch, &codec, &mut world, &mut watch, &size, first).await {
            Ok(true) => {}
            Ok(false) => sleep(idle).await,
            Err(e) => {
                eprintln!("{}: {e:#}; retrying in {idle:?}", chain.name);
                sleep(idle).await;
            }
        }
    }
}

/// Walks from the raw cursor to the settled block. `Ok(false)` when there was nothing to do.
/// Everything it writes is keyed on the natural key, so a round that fails half way is simply
/// run again.
#[allow(clippy::too_many_arguments)]
async fn round(
    chain: &registry::Chain,
    rpc: &Rpc,
    ch: &ClickHouse,
    codec: &Codec,
    world: &mut World,
    watch: &mut BTreeSet<String>,
    size: &AtomicU64,
    first: u64,
) -> Result<bool> {
    let id = chain.chain_id;
    // A settled block must never change. If the last one we recorded did, this chain stops here
    // until someone looks: see "Reorgs" in docs/data-pipeline.md.
    if let Some((number, hash)) = ch.settled_hash(id).await? {
        let now = header(&rpc.call("eth_getBlockByNumber", json!([format!("0x{number:x}"), false])).await?)?.hash;
        if now != hash {
            bail!("settled block {number} changed: stored {hash}, the node now says {now}. Not fetching further");
        }
    }
    // A Hub admitted elsewhere (the tip saw it first, or a candidate passed its second look) is
    // in meta.hubs but not watched yet: its logs so far are fetched before the walk goes on.
    if !ch.hubs(id).await?.is_subset(&world.hubs) {
        *world = ch.world(chain, codec, u64::MAX).await?;
    }
    let unwatched: Vec<String> = world.watch().difference(watch).cloned().collect();
    let upto = ch.cursor(id, "raw").await?.unwrap_or(0);
    adopt(chain, rpc, ch, codec, world, watch, unwatched, upto, true).await?;

    let start = ch.cursor(id, "raw").await?.map_or(first, |c| c + 1);
    let settled = header(&rpc.call("eth_getBlockByNumber", json!([chain.settle_tag, false])).await?)?;
    if settled.number < start {
        return Ok(false);
    }

    let addresses: Vec<String> = watch.iter().cloned().collect();
    let mut next = start;
    let ranges = std::iter::from_fn(|| {
        (next <= settled.number).then(|| {
            let from = next;
            let to = (from + size.load(Relaxed) - 1).min(settled.number);
            next = to + 1;
            (from, to)
        })
    });
    // Fetched in parallel, handed back in block order: the cursor only ever moves past a range
    // when every range before it is stored.
    let mut fetched = stream::iter(ranges)
        .map(|(from, to)| {
            let addresses = &addresses;
            // The second request has no address: any contract that emits AddAsset is a Hub candidate.
            async move { (to, futures::join!(rpc.logs(addresses, None, from, to, size), rpc.logs(&[], Some(crate::hubs::add_asset()), from, to, size))) }
        })
        .buffered(env_u64("FETCH_PARALLEL", 4) as usize);

    let (mut stored, mut found) = (0usize, Vec::new());
    while let Some((to, (logs, announced))) = fetched.next().await {
        crate::hubs::consider(chain, rpc, ch, &mut world.hubs, &announced).await?;
        found = new_addresses(codec, world, &logs, watch);
        if !found.is_empty() {
            break; // this range is not stored; it is fetched again with the new addresses
        }
        stored += store(id, rpc, ch, &logs).await?;
        let hash = if to == settled.number { settled.hash.as_str() } else { "" };
        ch.insert(
            "meta.cursor",
            &[
                json!({"chain_id": id, "stream": "raw", "block_number": to, "block_hash": hash}),
                json!({"chain_id": id, "stream": "hubscan", "block_number": to, "block_hash": ""}),
            ],
        )
        .await?;
        // Creep back up after a node made us shrink.
        size.store((size.load(Relaxed) * 11 / 10 + 1).min(MAX_RANGE), Relaxed);
    }
    drop(fetched);

    let upto = ch.cursor(id, "raw").await?.unwrap_or(0);
    adopt(chain, rpc, ch, codec, world, watch, found, upto, true).await?;
    if stored > 0 || settled.number - start >= 1_000 {
        let at = ch.cursor(id, "raw").await?.unwrap_or(0);
        eprintln!("{}: {stored} logs stored, raw cursor at {at} (settled {})", chain.name, settled.number);
    }
    Ok(true)
}

/// Writes logs to `raw.logs` (and the headers fetched for them to `raw.blocks`). Returns how many.
async fn store(chain_id: u32, rpc: &Rpc, ch: &ClickHouse, logs: &[Value]) -> Result<usize> {
    let (times, headers) = rpc.block_times(logs).await?;
    let rows = logs
        .iter()
        .map(|l| {
            let number = hex_u64(&l["blockNumber"])?;
            log_row(chain_id, l, *times.get(&number).context("log without a block time")?)
        })
        .collect::<Result<Vec<_>>>()?;
    ch.insert("raw.logs", &rows).await?;
    ch.insert("raw.blocks", &headers.iter().map(|h| h.row(chain_id)).collect::<Vec<_>>()).await?;
    Ok(rows.len())
}

/// Starts watching contracts found after the walk passed their first logs: a new Hub, and whatever
/// its logs announce in turn. A contract is deployed and configured before anything announces it,
/// so its logs are fetched for it alone, from the chain's start block up to `upto`, before the
/// walk goes on. Rare, and one request per MAX_RANGE blocks.
#[allow(clippy::too_many_arguments)]
async fn adopt(
    chain: &registry::Chain,
    rpc: &Rpc,
    ch: &ClickHouse,
    codec: &Codec,
    world: &mut World,
    watch: &mut BTreeSet<String>,
    mut found: Vec<String>,
    upto: u64,
    watching: bool,
) -> Result<()> {
    while let Some(address) = found.pop() {
        if watch.contains(&address) {
            continue;
        }
        let mut new = vec![address];
        new.extend(rpc.view_address(&new[0], ORACLE_SELECTOR).await.filter(|oracle| !watch.contains(oracle)));
        let earlier = history(chain.chain_id, rpc, ch, codec, world, &new, chain.start_block, upto, watching).await?;
        eprintln!("{}: new contract {}, {earlier} earlier logs stored", chain.name, new[0]);
        watch.extend(new);
        found.extend(world.watch().difference(watch).cloned());
    }
    Ok(())
}

/// Every log of `addresses` up to `upto`, stored and shown to `world`. It then marks each address:
/// `late:` says its logs up to that block arrived behind the cursors, so the calls stage and the
/// processor go back for them (calls.rs and process.rs, `late`); `watch:`, written only by a
/// process that goes on watching, says the walk has it from there, so the next start does not
/// fetch it again. The one-off backfill writes `late:` alone, and the walk's next start fetches
/// what came after it.
#[allow(clippy::too_many_arguments)]
async fn history(
    chain_id: u32,
    rpc: &Rpc,
    ch: &ClickHouse,
    codec: &Codec,
    world: &mut World,
    addresses: &[String],
    first: u64,
    upto: u64,
    watching: bool,
) -> Result<usize> {
    let late = ch.marks(chain_id, "late:").await?;
    let mut from = addresses.iter().map(|a| late.get(a).map_or(first, |block| block + 1)).min().unwrap_or(first);
    let (size, mut stored) = (AtomicU64::new(MAX_RANGE), 0);
    while from <= upto {
        let to = (from + size.load(Relaxed) - 1).min(upto);
        let logs = rpc.logs(addresses, None, from, to, &size).await;
        apply(codec, world, &logs);
        stored += store(chain_id, rpc, ch, &logs).await?;
        from = to + 1;
    }
    let marks: &[&str] = if watching { &["late", "watch"] } else { &["late"] };
    let done: Vec<Value> = addresses
        .iter()
        .flat_map(|a| marks.iter().map(move |mark| json!({"chain_id": chain_id, "stream": format!("{mark}:{a}"), "block_number": upto, "block_hash": ""})))
        .collect();
    ch.insert("meta.cursor", &done).await?;
    Ok(stored)
}

/// The contracts whose logs are fetched: what the stored events name (`World::watch`), plus the
/// seed. `registry/spokes.toml` is a seed, not the list: its Spokes are watched from the first
/// block so their history costs no extra request; anything else is found from events.
///
/// Only the seed is taken to be complete from the first block. Any other contract (a discovered
/// Hub or Spoke, an interest rate strategy, an access manager) was announced in logs stored before
/// it was watched: unless a `watch:` mark says the walk already has it, its logs up to `upto` are
/// fetched here, once. `watching` is false for the one-off backfill, which does not go on walking.
pub(crate) async fn watch_list(
    chain: &registry::Chain,
    rpc: &Rpc,
    ch: &ClickHouse,
    codec: &Codec,
    world: &mut World,
    upto: u64,
    watching: bool,
) -> Result<BTreeSet<String>> {
    let id = chain.chain_id;
    let mut watch: BTreeSet<String> = chain.hubs.iter().map(|h| h.address.to_lowercase()).collect();
    // FETCH_NO_SEED=1 ignores the seed, so every Spoke has to be discovered. For testing discovery.
    let seed = if env_u64("FETCH_NO_SEED", 0) == 1 { Vec::new() } else { registry::spokes() };
    for s in seed.into_iter().filter(|s| s.chain_id == id) {
        watch.insert(s.address.to_lowercase());
        watch.extend(s.oracle.map(|o| o.to_lowercase()));
    }
    *world = ch.world(chain, codec, u64::MAX).await?;
    let marks = ch.marks(id, "watch:").await?;
    let found: Vec<String> = world.watch().difference(&watch).filter(|a| !marks.contains_key(*a)).cloned().collect();
    watch.extend(world.watch().into_iter().filter(|a| !found.contains(a)));
    if upto > 0 {
        adopt(chain, rpc, ch, codec, world, &mut watch, found, upto, watching).await?;
    }
    Ok(watch)
}

/// Shows `world` the events in these logs (as the node returned them).
fn apply(codec: &Codec, world: &mut World, logs: &[Value]) {
    for log in logs {
        let topics: Vec<&str> = log["topics"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let (Some(address), Some(data)) = (log["address"].as_str(), log["data"].as_str()) else { continue };
        if let Some((_, name, args)) = codec.log(&topics, data) {
            world.apply(&address.to_lowercase(), name, &args);
        }
    }
}

/// Contracts named by the events in these logs that are not watched yet.
fn new_addresses(codec: &Codec, world: &mut World, logs: &[Value], watch: &BTreeSet<String>) -> Vec<String> {
    apply(codec, world, logs);
    world.watch().difference(watch).cloned().collect()
}

/// SQL condition for the events `World` is rebuilt from.
pub(crate) fn registry_events() -> String {
    let topics: Vec<String> =
        crate::model::REGISTRY_EVENTS.iter().filter_map(|(kind, event)| abi::topic0(kind, event)).map(|t| format!("'{t}'")).collect();
    format!("topic0 IN ({})", topics.join(","))
}

/// The address in a 32-byte topic or call result.
fn topic_address(word: &str) -> String {
    format!("0x{}", &word[word.len().saturating_sub(40)..]).to_lowercase()
}

/// A `raw.logs` row: the log as the node returned it, hex lowercased.
pub(crate) fn log_row(chain_id: u32, log: &Value, block_time: u64) -> Result<Value> {
    let text = |key: &str| {
        log[key].as_str().map(str::to_lowercase).with_context(|| format!("log without {key}"))
    };
    let topic = |i: usize| log["topics"][i].as_str().unwrap_or("").to_lowercase();
    Ok(json!({
        "chain_id": chain_id,
        "block_number": hex_u64(&log["blockNumber"])?,
        "block_time": block_time,
        "block_hash": text("blockHash")?,
        "tx_hash": text("transactionHash")?,
        "tx_index": hex_u64(&log["transactionIndex"])?,
        "log_index": hex_u64(&log["logIndex"])?,
        "address": text("address")?,
        "topic0": topic(0),
        "topic1": topic(1),
        "topic2": topic(2),
        "topic3": topic(3),
        "data": text("data")?,
    }))
}

pub(crate) struct Header {
    pub(crate) number: u64,
    pub(crate) hash: String,
    pub(crate) parent_hash: String,
    pub(crate) time: u64,
}

impl Header {
    fn row(&self, chain_id: u32) -> Value {
        json!({"chain_id": chain_id, "number": self.number, "hash": self.hash,
               "parent_hash": self.parent_hash, "block_time": self.time})
    }
}

pub(crate) fn header(block: &Value) -> Result<Header> {
    let text = |key: &str| {
        block[key].as_str().map(str::to_lowercase).with_context(|| format!("block without {key}"))
    };
    Ok(Header {
        number: hex_u64(&block["number"])?,
        hash: text("hash")?,
        parent_hash: text("parentHash")?,
        time: hex_u64(&block["timestamp"])?,
    })
}

fn hex_u64(v: &Value) -> Result<u64> {
    let s = v.as_str().with_context(|| format!("expected a hex quantity, got {v}"))?;
    u64::from_str_radix(s.trim_start_matches("0x"), 16).with_context(|| format!("bad quantity {s}"))
}

pub(crate) fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn short(e: impl std::fmt::Display) -> String {
    e.to_string().chars().take(200).collect()
}

#[derive(Clone)]
pub(crate) struct Rpc {
    http: reqwest::Client,
    url: String,
}

impl Rpc {
    pub(crate) fn new(url: String) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(60)).build().expect("http client");
        Self { http, url }
    }

    /// Sends one request or one batch. Retries what a retry can fix (transport, 429, 5xx) a few
    /// times; any other failure goes back to the caller, who may ask for less.
    async fn post(&self, body: &Value) -> Result<Value> {
        let mut wait = Duration::from_millis(500);
        for _ in 0..5 {
            let why = match self.http.post(&self.url).json(body).send().await {
                Ok(r) if r.status().is_success() => return Ok(r.json().await?),
                Ok(r) if r.status().as_u16() == 429 || r.status().is_server_error() => r.status().to_string(),
                Ok(r) => bail!("http {}: {}", r.status(), short(r.text().await.unwrap_or_default())),
                Err(e) => short(e),
            };
            eprintln!("rpc: {why}; retrying in {wait:?}");
            sleep(wait).await;
            wait *= 2;
        }
        bail!("rpc: gave up after 5 attempts")
    }

    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let reply = self.post(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).await?;
        if let Some(e) = reply.get("error") {
            bail!("{method}: {}", short(e));
        }
        reply.get("result").filter(|r| !r.is_null()).cloned().ok_or_else(|| anyhow!("{method}: no result"))
    }

    /// Every log of `addresses` (of any contract when there are none) with that `topic` (any when
    /// `None`) in `[from, to]`, in block order. When the node refuses a range it
    /// is halved, and `size` is lowered so the ranges that follow start smaller. A single block
    /// that keeps failing is retried until it answers: skipping it would leave a hole.
    pub(crate) async fn logs(&self, addresses: &[String], topic: Option<&str>, from: u64, to: u64, size: &AtomicU64) -> Vec<Value> {
        let mut out = Vec::new();
        let mut todo = vec![(from, to)];
        let mut wait = Duration::from_secs(1);
        while let Some((a, b)) = todo.pop() {
            let reply = self.logs_once(addresses, topic, a, b).await;
            match reply {
                Ok(logs) => out.extend(logs),
                Err(e) if a == b => {
                    eprintln!("rpc: block {a}: {}; retrying in {wait:?}", short(e));
                    sleep(wait).await;
                    wait = (wait * 2).min(Duration::from_secs(60));
                    todo.push((a, b));
                }
                Err(e) => {
                    if (a, b) == (from, to) {
                        eprintln!("rpc: blocks {a} to {b} refused ({}); asking for less", short(e));
                    }
                    let mid = a + (b - a) / 2;
                    todo.push((mid + 1, b));
                    todo.push((a, mid)); // popped first: block order is kept
                    size.store(((b - a + 1) / 2).max(1), Relaxed);
                }
            }
        }
        out
    }

    /// One range, one `eth_getLogs` per `FETCH_MAX_ADDRESSES` addresses (0, the default: a single
    /// request for all of them). Public nodes cap the filter at about 10 addresses; a paid
    /// endpoint with no cap costs one request per range.
    pub(crate) async fn logs_once(&self, addresses: &[String], topic: Option<&str>, from: u64, to: u64) -> Result<Vec<Value>> {
        let per_request = match env_u64("FETCH_MAX_ADDRESSES", 0) {
            0 => addresses.len().max(1),
            n => n as usize,
        };
        // No address is one request for the whole chain: only ever with a topic.
        let chunks: Vec<&[String]> = match (addresses.is_empty(), topic) {
            (true, None) => Vec::new(),
            (true, Some(_)) => vec![&[]],
            (false, _) => addresses.chunks(per_request).collect(),
        };
        let mut out = Vec::new();
        for chunk in chunks {
            let mut filter = json!({"fromBlock": format!("0x{from:x}"), "toBlock": format!("0x{to:x}")});
            if !chunk.is_empty() {
                filter["address"] = json!(chunk);
            }
            if let Some(topic) = topic {
                filter["topics"] = json!([topic]);
            }
            match self.call("eth_getLogs", json!([filter])).await? {
                Value::Array(logs) => out.extend(logs),
                other => bail!("eth_getLogs returned {}", short(other)),
            }
        }
        if addresses.len() > per_request {
            // several requests: put the logs back in chain order
            let mut keyed = out
                .into_iter()
                .map(|l| Ok(((hex_u64(&l["blockNumber"])?, hex_u64(&l["logIndex"])?), l)))
                .collect::<Result<Vec<_>>>()?;
            keyed.sort_by_key(|(key, _)| *key);
            out = keyed.into_iter().map(|(_, l)| l).collect();
        }
        Ok(out)
    }

    /// Block time for every block that has a log. Free when the node puts `blockTimestamp` on the
    /// log; otherwise one batched header request, and those headers go to `raw.blocks`.
    pub(crate) async fn block_times(&self, logs: &[Value]) -> Result<(HashMap<u64, u64>, Vec<Header>)> {
        let mut times = HashMap::new();
        let mut missing = BTreeSet::new();
        for log in logs {
            let number = hex_u64(&log["blockNumber"])?;
            match log.get("blockTimestamp").filter(|t| t.is_string()) {
                Some(t) => drop(times.insert(number, hex_u64(t)?)),
                None => drop(missing.insert(number)),
            }
        }
        let mut headers = Vec::new();
        let missing: Vec<u64> = missing.into_iter().filter(|n| !times.contains_key(n)).collect();
        // shortcut: assumes the node accepts JSON-RPC batches; add a one-by-one fallback if a
        // provider turns out not to.
        for chunk in missing.chunks(50) {
            let batch: Vec<Value> = chunk
                .iter()
                .map(|n| json!({"jsonrpc": "2.0", "id": n, "method": "eth_getBlockByNumber", "params": [format!("0x{n:x}"), false]}))
                .collect();
            let batch = Value::Array(batch);
            let mut wait = Duration::from_millis(500);
            // Each item counts against the provider's rate limit, and a limited item fails alone.
            // Ask for the batch again rather than fail the round and fetch its logs twice.
            let found = loop {
                let reply = self.post(&batch).await?;
                let items = reply.as_array().context("batch reply is not an array")?;
                match items.iter().map(|i| header(&i["result"])).collect::<Result<Vec<_>>>() {
                    Ok(found) if found.len() == chunk.len() => break found,
                    _ if wait > Duration::from_secs(30) => bail!("headers: {}", short(&reply)),
                    _ => {
                        sleep(wait).await;
                        wait *= 2;
                    }
                }
            };
            for h in found {
                times.insert(h.number, h.time);
                headers.push(h);
            }
        }
        Ok((times, headers))
    }

    /// The address a view with no arguments answers now (a standard Spoke's `ORACLE()`, a Hub's
    /// `authority()`); `None` when the contract has no such view or answers zero.
    pub(crate) async fn view_address(&self, contract: &str, selector: &str) -> Option<String> {
        let reply = self.call("eth_call", json!([{"to": contract, "data": selector}, "latest"])).await.ok()?;
        let word = reply.as_str().filter(|w| w.len() == 66)?;
        let answer = topic_address(word);
        (answer != format!("0x{}", "0".repeat(40))).then_some(answer)
    }
}

/// ClickHouse over its HTTP interface. Inserts are JSONEachRow and acknowledged when written.
#[derive(Clone)]
pub(crate) struct ClickHouse {
    http: reqwest::Client,
    url: String,
}

impl ClickHouse {
    pub(crate) fn new() -> Self {
        let url = env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into());
        Self { http: reqwest::Client::new(), url }
    }

    pub(crate) async fn query(&self, sql: &str, body: String) -> Result<String> {
        // A query with no data goes in the body: ClickHouse refuses a POST with an empty one.
        let request = match body.is_empty() {
            true => self.http.post(&self.url).body(sql.to_string()),
            false => self.http.post(&self.url).query(&[("query", sql)]).body(body),
        };
        let reply = request.send().await?;
        let ok = reply.status().is_success();
        let text = reply.text().await?;
        if !ok {
            bail!("clickhouse: {}", short(text.trim()));
        }
        Ok(text)
    }

    pub(crate) async fn insert(&self, table: &str, rows: &[Value]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let body = rows.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        self.query(&format!("INSERT INTO {table} FORMAT JSONEachRow"), body).await.map(drop)
    }

    /// Last block a stream (`raw`, `calls`, `core`) has completed, if it has started.
    pub(crate) async fn cursor(&self, chain_id: u32, stream: &str) -> Result<Option<u64>> {
        // max(): the cursor only moves forward, and two writes can share a timestamp.
        let sql = format!(
            "SELECT max(block_number), count() FROM meta.cursor WHERE chain_id = {chain_id} AND stream = '{stream}' FORMAT TSV"
        );
        let text = self.query(&sql, String::new()).await?;
        let mut cols = text.split_whitespace();
        let (max, count) = (cols.next().context("cursor query")?, cols.next().context("cursor query")?);
        Ok((count != "0").then(|| max.parse()).transpose()?)
    }

    /// The newest settled block whose hash the cursor recorded.
    async fn settled_hash(&self, chain_id: u32) -> Result<Option<(u64, String)>> {
        let sql = format!(
            "SELECT block_number, block_hash FROM meta.cursor WHERE chain_id = {chain_id} AND stream = 'raw' AND block_hash != '' \
             ORDER BY block_number DESC LIMIT 1 FORMAT TSV"
        );
        let text = self.query(&sql, String::new()).await?;
        Ok(text.trim().split_once('\t').and_then(|(number, hash)| Some((number.parse().ok()?, hash.to_string()))))
    }

    /// The Hubs that are synced on a chain: the seed (`load-registry`) and every Hub admitted
    /// since (hubs.rs). The one list every stage reads.
    pub(crate) async fn hubs(&self, chain_id: u32) -> Result<BTreeSet<String>> {
        let sql = format!("SELECT address FROM meta.hubs FINAL WHERE chain_id = {chain_id} FORMAT TSV");
        Ok(self.query(&sql, String::new()).await?.lines().map(str::to_string).collect())
    }

    /// What the events stored up to block `upto` say exists, on top of the Hub list: the seed in
    /// the binary plus `meta.hubs`. Every stage builds its `World` here.
    pub(crate) async fn world(&self, chain: &registry::Chain, codec: &Codec, upto: u64) -> Result<World> {
        let mut world = World::new(chain);
        world.hubs.extend(self.hubs(chain.chain_id).await?);
        for log in self.logs(chain.chain_id, chain.start_block, upto, &registry_events()).await? {
            if let Some((_, name, args)) = crate::model::decode(codec, &log) {
                world.apply(crate::model::text(&log, "address"), name, &args);
            }
        }
        Ok(world)
    }

    /// The marks of one kind in `meta.cursor` (`watch:`, `late:`, `late-calls:`, `late-core:`):
    /// address -> block.
    pub(crate) async fn marks(&self, chain_id: u32, prefix: &str) -> Result<BTreeMap<String, u64>> {
        let sql = format!(
            "SELECT substring(stream, {}), max(block_number) FROM meta.cursor \
             WHERE chain_id = {chain_id} AND startsWith(stream, '{prefix}') GROUP BY stream FORMAT TSV",
            prefix.len() + 1
        );
        let text = self.query(&sql, String::new()).await?;
        text.lines().filter_map(|l| l.split_once('\t')).map(|(address, block)| Ok((address.to_string(), block.parse()?))).collect()
    }

    /// Contracts one stage has finished up to a block (its `ready` marks) and the next has not
    /// (its `done` marks): address -> (done up to, 0 when never; ready up to).
    pub(crate) async fn pending(&self, chain_id: u32, ready: &str, done: &str) -> Result<crate::calls::Late> {
        let done = self.marks(chain_id, done).await?;
        let ready = self.marks(chain_id, ready).await?;
        Ok(ready.into_iter().filter(|(a, upto)| done.get(a).is_none_or(|at| at < upto)).map(|(a, upto)| (done.get(&a).copied().unwrap_or(0), a, upto)).map(|(at, a, upto)| (a, (at, upto))).collect())
    }

    /// Stored logs of `[from, to]` in chain order, each once. `only` is an SQL condition that
    /// narrows them (`registry_events()`), or empty.
    pub(crate) async fn logs(&self, chain_id: u32, from: u64, to: u64, only: &str) -> Result<Vec<Value>> {
        let only = if only.is_empty() { String::new() } else { format!("AND ({only})") };
        let sql = format!(
            "SELECT block_number, toUnixTimestamp(block_time) AS t, log_index, tx_hash, address, topic0, topic1, topic2, topic3, data \
             FROM raw.logs WHERE chain_id = {chain_id} AND block_number BETWEEN {from} AND {to} {only} \
             ORDER BY block_number, log_index LIMIT 1 BY block_number, log_index FORMAT JSONEachRow"
        );
        self.query(&sql, String::new()).await?.lines().map(|l| Ok(serde_json::from_str(l)?)).collect()
    }

    /// The last block that has stored calls at or below `upto`, and its time. `only` is an SQL
    /// condition that narrows the calls, or empty.
    pub(crate) async fn last_call(&self, chain_id: u32, upto: u64, only: &str) -> Result<Option<(u64, u64)>> {
        let only = if only.is_empty() { String::new() } else { format!("AND ({only})") };
        let sql = format!(
            "SELECT max(block_number), toUnixTimestamp(max(block_time)), count() FROM raw.calls \
             WHERE chain_id = {chain_id} AND block_number <= {upto} {only} FORMAT TSV"
        );
        let text = self.query(&sql, String::new()).await?;
        let cols: Vec<u64> = text.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
        Ok((cols.len() == 3 && cols[2] > 0).then(|| (cols[0], cols[1])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(topic0: &str, spoke_topic: &str) -> Value {
        json!({
            "address": "0xCca852Bc40e560adC3b1Cc58CA5b55638ce826c9",
            "topics": [topic0, "0x0000000000000000000000000000000000000000000000000000000000000003", spoke_topic],
            "data": "0x",
            "blockNumber": "0x1793a9b",
            "blockHash": "0xAB",
            "transactionHash": "0xCD",
            "transactionIndex": "0x2",
            "logIndex": "0x1f"
        })
    }

    #[test]
    fn a_log_becomes_a_lowercase_row() {
        let row = log_row(1, &log("0xAA", "0xBB"), 1_800_000_000).unwrap();
        assert_eq!(row["block_number"], 24_722_075);
        assert_eq!(row["log_index"], 31);
        assert_eq!(row["address"], "0xcca852bc40e560adc3b1cc58ca5b55638ce826c9");
        assert_eq!(row["topic0"], "0xaa");
        assert_eq!(row["topic3"], ""); // absent topics are empty, not missing
        assert_eq!(row["block_time"], 1_800_000_000u64);
        // a log the node truncated is an error, never a silent empty row
        assert!(log_row(1, &json!({"topics": []}), 0).is_err());
    }

    #[test]
    fn events_name_contracts_not_yet_watched() {
        let codec = Codec::new();
        let chain = registry::chains().into_iter().find(|c| c.chain_id == 1).unwrap();
        let mut world = World::new(&chain);
        let add_spoke = abi::topic0("hub", "AddSpoke").unwrap();
        let known = "0x000000000000000000000000092508a2Eb395cb3861a3fdB65405744d706476f";
        let new = "0x0000000000000000000000001111111111111111111111111111111111111111";
        let logs = [log(add_spoke, known), log(add_spoke, new), log(add_spoke, new), log("0xother", new)];
        let mut watch = world.watch();
        watch.insert("0x092508a2eb395cb3861a3fdb65405744d706476f".to_string());
        assert_eq!(new_addresses(&codec, &mut world, &logs, &watch), ["0x1111111111111111111111111111111111111111"]);
        watch.insert("0x1111111111111111111111111111111111111111".to_string());
        assert!(new_addresses(&codec, &mut world, &logs, &watch).is_empty());
    }
}
