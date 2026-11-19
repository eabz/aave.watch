//! Hub discovery. Spec: docs/data-pipeline.md, "Hub discovery".
//!
//! A Hub announces itself: it emits `AddAsset`. The walk (fetch.rs) and the tip (tip.rs) ask the
//! node for that event from any contract, and every emitter that is not a synced Hub is a
//! candidate. `admission` decides whether a candidate is synced; `meta.hubs` is the list of those
//! that are, and `meta.hub_candidates` records every decision.
//!
//! Also here: the one-off backfill of Hubs created behind the cursors (`backfill-hubs`), and the
//! task that keeps names and the Spoke rows of `meta.spokes` up to date.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::{abi, registry};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::fetch::{ClickHouse, Rpc};
use crate::model::World;

/// `authority()`: the access manager of a Hub.
const AUTHORITY_SELECTOR: &str = "0xbf7e214f";
/// Aave's app API: the Hubs and Spokes Aave lists, with their names. Not ours and not promised
/// to anyone: every use below works without it.
const AAVE_API: &str = "https://api.aave.com/internal/v4/graphql";
/// How often names are refreshed and candidates looked at again.
const REFRESH: Duration = Duration::from_secs(600);

/// topic0 of `AddAsset(uint256,address,uint8)`.
pub(crate) fn add_asset() -> &'static str {
    abi::topic0("hub", "AddAsset").expect("AddAsset in topics.tsv")
}

/// The admission rule: is a contract that emitted `AddAsset` synced as a Hub?
/// `Ok(why)` when it is, `Err(why not)` when it is only recorded as a candidate.
///
/// Anyone can deploy the open-source Hub, so announcing itself is not enough. A candidate is
/// admitted when its `authority()` is the authority of a Hub already synced on its chain, or when
/// Aave's API lists its address as a Hub (`listed`; `None` when the API did not answer).
/// `HUB_ADMIT=all` admits every candidate. This is the owner's rule and the only place it lives.
pub(crate) fn admission(all: bool, authority: &str, synced: &BTreeSet<String>, listed: Option<bool>) -> Result<String, String> {
    if all {
        return Ok("HUB_ADMIT=all".into());
    }
    if !authority.is_empty() && synced.contains(authority) {
        return Ok(format!("its authority {authority} is the authority of a Hub already synced"));
    }
    let authority = if authority.is_empty() { "it has no authority".to_string() } else { format!("its authority {authority} is not that of a synced Hub") };
    match listed {
        Some(true) => Ok("Aave's API lists it".into()),
        Some(false) => Err(format!("{authority} and Aave's API does not list it")),
        None => Err(format!("{authority} and Aave's API did not answer")),
    }
}

/// Candidates in `logs` (as the node returned them): the contracts that emit `AddAsset` and are
/// not synced Hubs, each with the block of its first one here.
pub(crate) fn announced(logs: &[Value], hubs: &BTreeSet<String>) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for log in logs {
        let (Some(address), Some(block)) = (log["address"].as_str(), log["blockNumber"].as_str()) else { continue };
        let Ok(block) = u64::from_str_radix(block.trim_start_matches("0x"), 16) else { continue };
        let address = address.to_lowercase();
        if log["topics"][0].as_str().is_some_and(|t| t.eq_ignore_ascii_case(add_asset())) && !hubs.contains(&address) {
            let first = out.entry(address).or_insert(block);
            *first = block.min(*first);
        }
    }
    out
}

/// What Aave's API lists: (chain id, address) -> name.
#[derive(Default)]
pub(crate) struct Listed {
    hubs: HashMap<(u32, String), String>,
    spokes: HashMap<(u32, String), String>,
}

/// The API's reply as `Listed`; `None` when it is not the shape we know.
fn parse(reply: &Value) -> Option<Listed> {
    let list = |key: &str| {
        let entry = |e: &Value| Some(((e["chain"]["chainId"].as_u64()? as u32, e["address"].as_str()?.to_lowercase()), e["name"].as_str()?.to_string()));
        reply["data"][key].as_array()?.iter().map(entry).collect::<Option<HashMap<_, _>>>()
    };
    Some(Listed { hubs: list("hubs")?, spokes: list("spokes")? })
}

/// One request to Aave's API. It carries the public query and nothing else.
async fn listed() -> Result<Listed> {
    let chains: Vec<String> = registry::chains().iter().map(|c| c.chain_id.to_string()).collect();
    let of = |what: &str| format!("{what}(request:{{query:{{chainIds:[{}]}}}}){{ name address chain{{chainId}} }}", chains.join(","));
    let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?;
    let reply = http.post(AAVE_API).json(&json!({"query": format!("{{ {} {} }}", of("hubs"), of("spokes"))})).send().await?.error_for_status()?;
    parse(&reply.json().await?).context("unexpected answer")
}

/// Decides on one candidate and records the decision in `meta.hub_candidates`; an admitted one is
/// added to `meta.hubs`, named when Aave's API names it. Says so in the log when the decision is
/// new or has changed. A node or an API that does not answer admits nothing: the candidate is
/// looked at again by `refresh`.
async fn admit(chain: &registry::Chain, rpc: &Rpc, ch: &ClickHouse, hubs: &BTreeSet<String>, listed: Option<&Listed>, candidate: &str, block: u64) -> Result<bool> {
    let id = chain.chain_id;
    if ch.hubs(id).await?.contains(candidate) {
        return Ok(true);
    }
    let authority = rpc.view_address(candidate, AUTHORITY_SELECTOR).await.unwrap_or_default();
    let mut synced = BTreeSet::new();
    for hub in hubs {
        synced.extend(rpc.view_address(hub, AUTHORITY_SELECTOR).await);
    }
    let name = listed.and_then(|l| l.hubs.get(&(id, candidate.to_string())));
    let all = std::env::var("HUB_ADMIT").is_ok_and(|v| v == "all");
    let verdict = admission(all, &authority, &synced, listed.map(|_| name.is_some()));
    let sql = format!("SELECT first_block, admitted FROM meta.hub_candidates FINAL WHERE chain_id = {id} AND address = '{candidate}' FORMAT TSV");
    let before: Vec<u64> = ch.query(&sql, String::new()).await?.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let first_block = before.first().map_or(block, |b| block.min(*b));
    let (admitted, reason) = match &verdict {
        Ok(why) => (true, why),
        Err(why) => (false, why),
    };
    if admitted {
        ch.insert("meta.hubs", &[json!({"chain_id": id, "address": candidate, "name": name.map_or("", String::as_str), "first_block": first_block})]).await?;
    }
    ch.insert(
        "meta.hub_candidates",
        &[json!({"chain_id": id, "address": candidate, "first_block": first_block, "authority": authority, "admitted": admitted as u8, "reason": reason})],
    )
    .await?;
    if before.get(1) != Some(&(admitted as u64)) {
        let what = if admitted { "new Hub" } else { "Hub candidate not synced:" };
        eprintln!("{}: {what} {candidate}, first AddAsset at block {first_block}: {reason}", chain.name);
    }
    Ok(admitted)
}

/// Looks at the candidates in `logs` and adds the admitted ones to `hubs`. Returns them.
pub(crate) async fn consider(chain: &registry::Chain, rpc: &Rpc, ch: &ClickHouse, hubs: &mut BTreeSet<String>, logs: &[Value]) -> Result<Vec<String>> {
    let candidates = announced(logs, hubs);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let listed = listed().await.ok();
    let mut admitted = Vec::new();
    for (candidate, block) in candidates {
        if admit(chain, rpc, ch, hubs, listed.as_ref(), &candidate, block).await? {
            hubs.insert(candidate.clone());
            admitted.push(candidate);
        }
    }
    Ok(admitted)
}

/// Looks for candidates in the blocks no walk has looked at: from the `hubscan` cursor (the
/// chain's start block when there is none) up to `upto`. One request per 10,000 blocks.
pub(crate) async fn scan(chain: &registry::Chain, rpc: &Rpc, ch: &ClickHouse, hubs: &mut BTreeSet<String>, upto: u64) -> Result<()> {
    let id = chain.chain_id;
    let mut from = ch.cursor(id, "hubscan").await?.map_or(chain.start_block, |c| c + 1);
    if from <= upto {
        eprintln!("{}: looking for Hubs in blocks {from} to {upto}", chain.name);
    }
    let size = AtomicU64::new(10_000);
    while from <= upto {
        let to = (from + size.load(Relaxed) - 1).min(upto);
        let logs = rpc.logs(&[], Some(add_asset()), from, to, &size).await;
        consider(chain, rpc, ch, hubs, &logs).await?;
        ch.insert("meta.cursor", &[json!({"chain_id": id, "stream": "hubscan", "block_number": to, "block_hash": ""})]).await?;
        from = to + 1;
    }
    Ok(())
}

/// `meta.spokes` rows for the Spokes that synced Hubs announced and that have none yet. The kind
/// is what the Spoke's own events say; the name is empty until `refresh` finds one.
fn spoke_rows(chain_id: u32, world: &World, known: &BTreeSet<String>) -> Vec<Value> {
    let spokes: BTreeSet<&String> = world.hub_spokes.iter().map(|(_, _, spoke)| spoke).filter(|spoke| !known.contains(*spoke)).collect();
    let row = |spoke: &String| {
        let kind = match () {
            _ if world.oracles.contains_key(spoke) => "spoke",
            _ if world.vaults.contains_key(spoke) => "tokenization",
            _ => "treasury",
        };
        json!({"chain_id": chain_id, "address": spoke, "name": "", "kind": kind, "oracle": world.oracles.get(spoke).map_or("", String::as_str), "first_block": 0})
    };
    spokes.into_iter().map(row).collect()
}

/// Gives every discovered Spoke of a chain its `meta.spokes` row, so the API lists it.
async fn list_spokes(chain: &registry::Chain, ch: &ClickHouse, codec: &Codec) -> Result<BTreeSet<String>> {
    let id = chain.chain_id;
    let world = ch.world(chain, codec, u64::MAX).await?;
    let sql = format!("SELECT address FROM meta.spokes FINAL WHERE chain_id = {id} FORMAT TSV");
    let known: BTreeSet<String> = ch.query(&sql, String::new()).await?.lines().map(str::to_string).collect();
    ch.insert("meta.spokes", &spoke_rows(id, &world, &known)).await?;
    Ok(world.hubs)
}

/// Fills the empty names of `meta.hubs` and `meta.spokes` with the names Aave's API gives. A name
/// that is set (from `registry/*.toml`, or filled here earlier) is never changed.
async fn fill_names(ch: &ClickHouse, listed: &Listed) -> Result<()> {
    for (table, names) in [("meta.hubs", &listed.hubs), ("meta.spokes", &listed.spokes)] {
        let sql = format!("SELECT * EXCEPT (updated_at) FROM {table} FINAL WHERE name = '' FORMAT JSONEachRow");
        let mut rows = Vec::new();
        for line in ch.query(&sql, String::new()).await?.lines() {
            let mut row: Value = serde_json::from_str(line)?;
            let key = (row["chain_id"].as_u64().unwrap_or(0) as u32, row["address"].as_str().unwrap_or("").to_string());
            if let Some(name) = names.get(&key) {
                eprintln!("names: {} {} on chain {} is {name}", &table[5..table.len() - 1], key.1, key.0);
                row["name"] = json!(name);
                rows.push(row);
            }
        }
        ch.insert(table, &rows).await?;
    }
    Ok(())
}

/// One round of upkeep for a chain: Spoke rows, and a second look at the candidates that were
/// not admitted (Aave may list a Hub after it is created). A Hub admitted here is picked up by
/// the walk's next round, which fetches its logs from the start block.
async fn upkeep(chain: &registry::Chain, rpc: &Rpc, ch: &ClickHouse, codec: &Codec, listed: Option<&Listed>) -> Result<()> {
    let hubs = list_spokes(chain, ch, codec).await?;
    let sql = format!("SELECT address, first_block FROM meta.hub_candidates FINAL WHERE chain_id = {} AND admitted = 0 FORMAT TSV", chain.chain_id);
    for line in ch.query(&sql, String::new()).await?.lines() {
        let (candidate, block) = line.split_once('\t').context("candidate row")?;
        admit(chain, rpc, ch, &hubs, listed, candidate, block.parse()?).await?;
    }
    Ok(())
}

/// The fetcher's upkeep task, every ten minutes. Aave's API not answering changes nothing and is
/// logged once per streak of failures, not every round.
pub async fn refresh(ch: ClickHouse, chains: Vec<(registry::Chain, Rpc)>) -> Result<()> {
    let codec = Codec::new();
    let mut failing = false;
    loop {
        let listed = match listed().await {
            Ok(listed) => {
                if failing {
                    eprintln!("names: Aave's API answers again");
                }
                failing = false;
                Some(listed)
            }
            Err(e) => {
                if !failing {
                    eprintln!("names: Aave's API did not answer ({e:#}); names and candidates stay as they are until it does");
                }
                failing = true;
                None
            }
        };
        for (chain, rpc) in &chains {
            if let Err(e) = upkeep(chain, rpc, &ch, &codec, listed.as_ref()).await {
                eprintln!("{}: upkeep: {e:#}", chain.name);
            }
        }
        if let Some(listed) = &listed {
            if let Err(e) = fill_names(&ch, listed).await {
                eprintln!("names: {e:#}");
            }
        }
        sleep(REFRESH).await;
    }
}

/// `backfill-hubs [chain_id [upto_block]]`: syncs the Hubs created behind the cursors, in this
/// process, with the code of the stages: the scan, the late logs (`watch_list`), their view calls
/// (`calls::late`), their rows (`process::late`), their names. Resumable and safe to run again:
/// every step marks what it has done in `meta.cursor`.
pub async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let usage = "usage: aave-watch-indexer backfill-hubs [chain_id [upto_block]]";
    let only: Option<u32> = args.first().map(|a| a.parse()).transpose().ok().context(usage)?;
    let limit: Option<u64> = args.get(1).map(|a| a.parse()).transpose().ok().context(usage)?;
    let ch = ClickHouse::new();
    let mut tasks = tokio::task::JoinSet::new();
    for chain in registry::chains().into_iter().filter(|c| only.is_none_or(|id| id == c.chain_id)) {
        let var = format!("RPC_HTTP_{}", chain.chain_id);
        match std::env::var(&var) {
            Ok(url) => drop(tasks.spawn(backfill(chain, Rpc::new(url), ch.clone(), limit))),
            Err(_) => eprintln!("{}: {var} is not set, chain skipped", chain.name),
        }
    }
    while let Some(ended) = tasks.join_next().await {
        ended??;
    }
    if let Ok(listed) = listed().await.inspect_err(|e| eprintln!("names: Aave's API did not answer ({e:#})")) {
        fill_names(&ch, &listed).await?;
    }
    Ok(())
}

async fn backfill(chain: registry::Chain, rpc: Rpc, ch: ClickHouse, limit: Option<u64>) -> Result<()> {
    let (id, codec) = (chain.chain_id, Codec::new());
    // Up to the core cursor, the lowest of the three: the running stages never read those blocks
    // again, so nothing written here is decoded a second time by them. What comes after is
    // fetched by the walk when it next starts.
    let upto = ch.cursor(id, "core").await?.unwrap_or(0).min(limit.unwrap_or(u64::MAX));
    let mut world = World::new(&chain);
    world.hubs.extend(ch.hubs(id).await?);
    scan(&chain, &rpc, &ch, &mut world.hubs, upto).await?;
    crate::fetch::watch_list(&chain, &rpc, &ch, &codec, &mut world, upto, false).await?;
    crate::calls::late(&chain, &rpc, &ch, &codec).await?;
    crate::process::late(&chain, &ch, &codec).await?;
    list_spokes(&chain, &ch, &codec).await?;
    eprintln!("{}: Hubs are synced up to block {upto}", chain.name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AAVE: &str = "0x08ae3be30958cdd1847ec58fffd4c451a87fdf01";
    const OTHER: &str = "0x911f0d5cdd8d26344c9d704c8680213318be1a84";

    #[test]
    fn a_candidate_is_admitted_by_authority_or_by_aaves_list() {
        let synced: BTreeSet<String> = [AAVE.to_string()].into();
        // the authority of a Hub already synced: admitted, whatever the API says or fails to say
        assert!(admission(false, AAVE, &synced, Some(false)).is_ok());
        assert!(admission(false, AAVE, &synced, None).is_ok());
        // another authority: only when Aave lists it
        assert!(admission(false, OTHER, &synced, Some(true)).unwrap().contains("lists"));
        assert!(admission(false, OTHER, &synced, Some(false)).unwrap_err().contains(OTHER));
        // the API did not answer: not admitted, and the reason says so (it is looked at again)
        assert!(admission(false, OTHER, &synced, None).unwrap_err().contains("did not answer"));
        // a contract with no authority() never matches, even when no Hub is synced
        assert!(admission(false, "", &synced, Some(false)).is_err());
        assert!(admission(false, "", &BTreeSet::new(), None).is_err());
        // HUB_ADMIT=all
        assert!(admission(true, "", &synced, Some(false)).is_ok());
    }

    #[test]
    fn candidates_are_the_unknown_contracts_that_emit_add_asset() {
        let log = |address: &str, topic: &str, block: &str| json!({"address": address, "topics": [topic], "blockNumber": block});
        let hub = "0xCca852Bc40e560adC3b1Cc58CA5b55638ce826c9";
        let hubs: BTreeSet<String> = [hub.to_lowercase()].into();
        let other = abi::topic0("hub", "AddSpoke").unwrap();
        let logs = [
            log(hub, add_asset(), "0x10"),                                           // a synced Hub
            log("0xD138FcB0E8C52fDbB2Ac4573FA4d8925cf11e9c2", add_asset(), "0x30"), // new, seen twice
            log("0xd138fcb0e8c52fdbb2ac4573fa4d8925cf11e9c2", &add_asset().to_uppercase().replace("0X", "0x"), "0x20"),
            log("0x1111111111111111111111111111111111111111", other, "0x20"),       // another event
            json!({"address": "0x2222222222222222222222222222222222222222", "topics": []}), // not a log we can read
        ];
        let found = announced(&logs, &hubs);
        assert_eq!(found.into_iter().collect::<Vec<_>>(), [("0xd138fcb0e8c52fdbb2ac4573fa4d8925cf11e9c2".to_string(), 0x20)]);
        assert!(announced(&[], &hubs).is_empty());
    }

    #[test]
    fn aaves_list_is_read_by_chain_and_lowercase_address() {
        let reply = json!({"data": {
            "hubs": [{"name": "Sentora", "address": "0xD138FcB0E8C52fDbB2Ac4573FA4d8925cf11e9c2", "chain": {"chainId": 1}}],
            "spokes": [{"name": "Main", "address": "0x435272CefF93a1E657E8ABfdf0A13e95900A3a56", "chain": {"chainId": 43114}}],
        }});
        let listed = parse(&reply).unwrap();
        assert_eq!(listed.hubs[&(1, "0xd138fcb0e8c52fdbb2ac4573fa4d8925cf11e9c2".to_string())], "Sentora");
        assert_eq!(listed.spokes[&(43114, "0x435272ceff93a1e657e8abfdf0a13e95900a3a56".to_string())], "Main");
        // the same address on another chain is another contract
        assert!(!listed.hubs.contains_key(&(8453, "0xd138fcb0e8c52fdbb2ac4573fa4d8925cf11e9c2".to_string())));
        // an error body, or a changed shape, is no list at all: never an empty one that says "not listed"
        assert!(parse(&json!({"errors": [{"message": "no"}]})).is_none());
        assert!(parse(&json!({"data": {"hubs": [{"name": "x"}], "spokes": []}})).is_none());
    }

    #[test]
    fn a_discovered_spoke_gets_a_row_with_its_kind_and_no_name() {
        let (hub, spoke, vault, treasury, oracle) = ("0xaa", "0xbb", "0xcc", "0xdd", "0xee");
        let mut world = World { hubs: [hub.to_string()].into(), ..World::default() };
        for s in [spoke, vault, treasury] {
            world.apply(hub, "AddSpoke", &json!({"assetId": "0", "spoke": s}));
        }
        world.apply(hub, "AddSpoke", &json!({"assetId": "1", "spoke": spoke}));
        world.apply(spoke, "SetSpokeImmutables", &json!({"oracle": oracle}));
        world.apply(vault, "SetTokenizationSpokeImmutables", &json!({"assetId": "0", "hub": hub}));
        let rows = spoke_rows(1, &world, &[treasury.to_string()].into());
        // one row per Spoke, not per line; the one that has a row keeps it
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0]["address"].as_str(), rows[0]["kind"].as_str(), rows[0]["oracle"].as_str()), (Some(spoke), Some("spoke"), Some(oracle)));
        assert_eq!((rows[1]["kind"].as_str(), rows[1]["name"].as_str()), (Some("tokenization"), Some("")));
        assert_eq!(spoke_rows(1, &world, &BTreeSet::new())[2]["kind"], "treasury");
    }
}
