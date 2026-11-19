//! `indexer process`: raw rows to `core.*`. Spec: docs/data-pipeline.md, stages 3 to 5.
//!
//! Never talks to a node. Per chain it walks from the `core` cursor to the `calls` cursor, reading
//! `raw.logs` and `raw.calls`, and writes events, state rows, prices and the registry tables.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::registry;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::fetch::ClickHouse;
use crate::model::{World, decode, int, text};

/// Bump when parsing or enrichment changes what a row says; old rows are then reparsed.
const PARSER_VERSION: u16 = 2;
/// `Value` in the Spoke's account data is USD in the oracle's decimals, scaled by WAD.
const WAD_DECIMALS: i32 = 18;
const RAY_DECIMALS: i32 = 27;

pub async fn run() -> Result<()> {
    let ch = ClickHouse::new();
    let mut tasks = tokio::task::JoinSet::new();
    for chain in registry::chains() {
        tasks.spawn(crate::live::run_chain(chain.clone(), ch.clone()));
        tasks.spawn(run_chain(chain, ch.clone()));
    }
    // A chain task loops forever; if one ends, stop so systemd restarts the process.
    match tasks.join_next().await {
        Some(ended) => ended?,
        None => bail!("no chain in the registry"),
    }
}

async fn run_chain(chain: registry::Chain, ch: ClickHouse) -> Result<()> {
    let (id, codec) = (chain.chain_id, Codec::new());
    let mut cursor = ch.cursor(id, "core").await?.unwrap_or(chain.start_block - 1);
    let mut world = ch.world(&chain, &codec, cursor).await?;
    // A chain processed from its first block (a first run, or a reparse) starts with no prices:
    // the latest ones belong to the future of the blocks it is about to read.
    let mut state = if cursor < chain.start_block { State::default() } else { State::load(&ch, id).await? };
    let idle = Duration::from_millis((chain.block_time_ms * 8).clamp(5_000, 120_000));
    // About ten hours of blocks per round.
    let chunk = 36_000_000 / chain.block_time_ms;
    eprintln!("{}: processing from block {}", chain.name, cursor + 1);
    loop {
        let to = ch.cursor(id, "calls").await?.unwrap_or(0).min(cursor + chunk);
        // After the cursor is read: a late contract is marked before the range that announced it
        // is stored, and its calls before the calls cursor passes that range.
        if late(&chain, &ch, &codec).await? {
            world = ch.world(&chain, &codec, cursor).await?;
            state = State::load(&ch, id).await?;
        }
        if to <= cursor {
            sleep(idle).await;
            continue;
        }
        let logs = ch.logs(id, cursor + 1, to, "").await?;
        let calls = ch.calls(id, cursor + 1, to, "").await?;
        let mut blocks: BTreeMap<u64, (Vec<&Value>, Vec<&Value>)> = BTreeMap::new();
        for log in &logs {
            blocks.entry(log["block_number"].as_u64().unwrap_or(0)).or_default().0.push(log);
        }
        for call in &calls {
            blocks.entry(call["block_number"].as_u64().unwrap_or(0)).or_default().1.push(call);
        }
        let mut out = Rows::default();
        for (block, (logs, calls)) in blocks {
            process_block(&codec, &mut world, &mut state, id, block, &logs, &calls, &mut out);
        }
        let written: usize = out.0.values().map(Vec::len).sum();
        for (table, rows) in &out.0 {
            ch.insert(table, rows).await?;
        }
        ch.insert("meta.cursor", &[json!({"chain_id": id, "stream": "core", "block_number": to, "block_hash": ""})]).await?;
        if written > 0 {
            eprintln!("{}: {written} rows written, core cursor at {to}", chain.name);
        }
        cursor = to;
    }
}

/// Decodes contracts that are late (calls.rs, `Late`) once their calls are read: every log and
/// call of theirs from the chain's start block, through the same `process_block`, into the same
/// tables. `Ok(false)` when nothing is late. Other contracts' registry events are replayed along,
/// so the registry at each block is the whole chain's; prices are those of the late contracts'
/// own oracles. For that reason a pass decodes every contract that was ever late, each up to its
/// mark, not only the new ones: a Hub's rows are priced by its Spokes, and the two are not always
/// marked in the same round. The rows are keyed on the block, so a later pass replaces the
/// earlier one's rows with the same rows and adds the new ones.
pub(crate) async fn late(chain: &registry::Chain, ch: &ClickHouse, codec: &Codec) -> Result<bool> {
    let id = chain.chain_id;
    if ch.pending(id, "late-calls:", "late-core:").await?.is_empty() {
        return Ok(false);
    }
    let late: crate::calls::Late = ch.marks(id, "late-calls:").await?.into_iter().map(|(a, upto)| (a, (0, upto))).collect();
    let upto = late.values().map(|(_, upto)| *upto).max().unwrap_or(0);
    let mine = |log: &Value| late.get(text(log, "address")).is_some_and(|(_, upto)| log["block_number"].as_u64() <= Some(*upto));
    let mut world = World::new(chain);
    world.hubs.extend(ch.hubs(id).await?);
    let mut state = State::default();
    let logs = ch.logs(id, chain.start_block, upto, &crate::calls::late_logs(&late)).await?;
    // Their calls, and the symbol and name of the tokens their Hubs list (read with the listing).
    let listed = logs.iter().filter(|log| mine(log)).filter_map(|log| decode(codec, log)).filter(|(_, name, _)| *name == "AddAsset");
    let targets: BTreeSet<String> = late.keys().cloned().chain(listed.map(|(_, _, args)| text(&args, "underlying").to_string())).collect();
    let targets: Vec<String> = targets.iter().map(|t| format!("'{t}'")).collect();
    let calls = ch.calls(id, chain.start_block, upto, &format!("target IN ({})", targets.join(","))).await?;
    let mut blocks: BTreeMap<u64, (Vec<&Value>, Vec<&Value>)> = BTreeMap::new();
    for log in &logs {
        blocks.entry(log["block_number"].as_u64().unwrap_or(0)).or_default().0.push(log);
    }
    for call in &calls {
        blocks.entry(call["block_number"].as_u64().unwrap_or(0)).or_default().1.push(call);
    }
    let (mut out, mut written) = (Rows::default(), 0);
    for (block, (logs, calls)) in blocks {
        let (logs, others): (Vec<&Value>, Vec<&Value>) = logs.into_iter().partition(|log| mine(log));
        for log in others {
            if let Some((_, name, args)) = decode(codec, log) {
                world.apply(text(log, "address"), name, &args);
            }
        }
        if !logs.is_empty() || !calls.is_empty() {
            process_block(codec, &mut world, &mut state, id, block, &logs, &calls, &mut out);
        }
        if out.0.values().map(Vec::len).sum::<usize>() >= 50_000 {
            written += out.flush(ch, &late).await?;
        }
    }
    written += out.flush(ch, &late).await?;
    let done: Vec<Value> =
        late.iter().map(|(a, (_, upto))| json!({"chain_id": id, "stream": format!("late-core:{a}"), "block_number": upto, "block_hash": ""})).collect();
    ch.insert("meta.cursor", &done).await?;
    eprintln!("{}: {} late contracts decoded up to block {upto}, {written} rows written", chain.name, late.len());
    Ok(true)
}

/// Rows to insert, by table.
#[derive(Default)]
pub struct Rows(pub BTreeMap<&'static str, Vec<Value>>);

impl Rows {
    fn push(&mut self, table: &'static str, row: Value) {
        self.0.entry(table).or_default().push(row);
    }

    /// Writes the rows of a late pass and empties `self`. The price of a Hub asset is the median
    /// over its Spokes, and a late pass knows only the late Spokes' prices: it writes that price
    /// for late Hubs and leaves every other Hub's alone.
    async fn flush(&mut self, ch: &ClickHouse, late: &crate::calls::Late) -> Result<usize> {
        let mut written = 0;
        for (table, mut rows) in std::mem::take(&mut self.0) {
            if table == "core.asset_prices" {
                rows.retain(|row| late.contains_key(text(row, "hub")));
            }
            ch.insert(table, &rows).await?;
            written += rows.len();
        }
        Ok(written)
    }
}

/// What enrichment remembers between blocks: the last price of everything.
#[derive(Default)]
pub struct State {
    /// (spoke, reserve id) -> (raw price as the oracle gave it, USD)
    prices: HashMap<(String, u32), (String, f64)>,
    /// (hub, asset id) -> USD: the median of its Spokes' prices
    asset_prices: HashMap<(String, u32), f64>,
    /// oracle -> decimals of its prices
    oracle_decimals: HashMap<String, i32>,
    /// (spoke, reserve id) -> when its price source last updated, as last written
    updated: HashMap<(String, u32), u64>,
}

impl State {
    pub(crate) async fn load(ch: &ClickHouse, chain_id: u32) -> Result<Self> {
        let mut state = Self::default();
        let rows = |text: String| text.lines().map(|l| l.split('\t').map(str::to_string).collect::<Vec<_>>()).collect::<Vec<_>>();
        let sql = format!("SELECT spoke, reserve_id, price_raw, price FROM core.prices_latest FINAL WHERE chain_id = {chain_id} FORMAT TSV");
        for r in rows(ch.query(&sql, String::new()).await?) {
            state.prices.insert((r[0].clone(), r[1].parse()?), (r[2].clone(), r[3].parse()?));
        }
        let sql = format!("SELECT hub, asset_id, price FROM core.asset_prices_latest FINAL WHERE chain_id = {chain_id} FORMAT TSV");
        for r in rows(ch.query(&sql, String::new()).await?) {
            state.asset_prices.insert((r[0].clone(), r[1].parse()?), r[2].parse()?);
        }
        // decimals() is only ever asked of oracles
        let sql = format!("SELECT DISTINCT target, result FROM raw.calls WHERE chain_id = {chain_id} AND selector = '0x313ce567' AND ok = 1 FORMAT TSV");
        for r in rows(ch.query(&sql, String::new()).await?) {
            state.oracle_decimals.insert(r[0].clone(), i32::from_str_radix(r[1].trim_start_matches("0x").trim_start_matches('0'), 16).unwrap_or(0));
        }
        Ok(state)
    }
}

/// (view name, target, inputs joined by ":") -> (output, why it was read)
type Views<'s> = HashMap<(&'s str, &'s str, String), (Value, &'s str)>;

fn view<'v, 's>(views: &'v Views<'s>, name: &'s str, target: &'s str, key: &str) -> Option<&'v Value> {
    views.get(&(name, target, key.to_string())).map(|(output, _)| output)
}

/// A decimal string as a float; 0 when absent.
fn num(v: &Value) -> f64 {
    v.as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

fn scaled(v: &Value, decimals: i32) -> f64 {
    num(v) / 10f64.powi(decimals)
}

fn flag(v: &Value) -> u8 {
    (*v == true) as u8
}

/// A raw integer for a UInt256 column: the decimal string, "0" when absent.
fn raw(v: &Value) -> &str {
    v.as_str().unwrap_or("0")
}

/// One block: the registry learns from its logs, prices from its calls, then events and state rows
/// are written with the prices of this same block.
#[allow(clippy::too_many_arguments)]
pub fn process_block(
    codec: &Codec,
    world: &mut World,
    state: &mut State,
    chain_id: u32,
    block: u64,
    logs: &[&Value],
    calls: &[&Value],
    out: &mut Rows,
) {
    let time = logs.iter().chain(calls).find_map(|r| r["t"].as_u64()).unwrap_or(0);
    let stamp = |row: &mut Value| {
        row["chain_id"] = json!(chain_id);
        row["block_number"] = json!(block);
        row["block_time"] = json!(time);
        row["parser_version"] = json!(PARSER_VERSION);
    };

    let events: Vec<(&Value, &'static str, &str, Value)> =
        logs.iter().filter_map(|log| decode(codec, log).map(|(kind, name, args)| (*log, kind, name, args))).collect();
    for (log, _, name, args) in &events {
        let contract = text(log, "address");
        world.apply(contract, name, args);
        match *name {
            "AddAsset" if world.hubs.contains(contract) => {
                if let Some(((_, id), (token, decimals))) = int(args, "assetId").and_then(|id| world.assets.get_key_value(&(contract.to_string(), id))) {
                    out.push("meta.assets", json!({"chain_id": chain_id, "hub": contract, "asset_id": id, "token": token, "decimals": decimals, "first_block": block}));
                }
            }
            "AddReserve" => {
                if let Some(((_, id), asset)) = int(args, "reserveId").and_then(|id| world.reserves.get_key_value(&(contract.to_string(), id))) {
                    let (token, decimals) = world.assets.get(asset).cloned().unwrap_or_default();
                    out.push("meta.reserves", json!({"chain_id": chain_id, "spoke": contract, "reserve_id": id, "hub": asset.0, "asset_id": asset.1,
                                                     "token": token, "decimals": decimals, "first_block": block}));
                }
            }
            _ => {}
        }
    }

    let mut views: Views = HashMap::new();
    for call in calls {
        if let Some((name, inputs, output)) = codec.result(text(call, "selector"), text(call, "args"), text(call, "result")) {
            let key = inputs.as_object().into_iter().flatten().filter_map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(":");
            views.insert((name, text(call, "target"), key), (output, text(call, "reason")));
        }
    }

    // Constants read once: oracle decimals, token symbol and name.
    for ((name, target, _), (output, _)) in &views {
        match *name {
            "decimals" => drop(state.oracle_decimals.insert(target.to_string(), num(output) as i32)),
            "symbol" => {
                let decimals = world.assets.values().find(|(token, _)| token == target).map_or(0, |(_, d)| *d);
                let name = view(&views, "name", target, "").and_then(Value::as_str).unwrap_or("");
                out.push("meta.tokens", json!({"chain_id": chain_id, "address": target, "symbol": output, "name": name, "decimals": decimals, "canonical": output.as_str().unwrap_or("").to_lowercase(), "logo": ""}));
            }
            _ => {}
        }
    }

    // Prices. A row only when the price or its source's update time changed; then one price per
    // Hub asset, the median.
    let mut repriced: BTreeSet<(String, u32)> = BTreeSet::new();
    for (spoke, oracle) in &world.oracles {
        let ids = world.reserve_ids(spoke);
        // Its one input is an array, so its key is empty.
        let Some(prices) = view(&views, "getReservesPrices", oracle, "") else { continue };
        let Some(prices) = prices.as_array().filter(|p| p.len() == ids.len()) else { continue };
        let decimals = state.oracle_decimals.get(oracle).copied().unwrap_or(8);
        for (id, price_raw) in ids.iter().zip(prices) {
            let at = (spoke.clone(), *id);
            // Where the source is a Chainlink feed it also says when it last updated (read hourly).
            let source = world.sources.get(&at).map_or("", String::as_str);
            let updated = view(&views, "latestRoundData", source, "").and_then(|round| round[3].as_str()?.parse::<u64>().ok());
            let same_price = state.prices.get(&at).is_some_and(|(old, _)| old == raw(price_raw));
            if same_price && (updated.is_none() || state.updated.get(&at) == updated.as_ref()) {
                continue;
            }
            state.updated.extend(updated.map(|u| (at.clone(), u)));
            let price = scaled(price_raw, decimals);
            let mut row = json!({"spoke": spoke, "reserve_id": id, "price_raw": raw(price_raw), "price": price,
                                 "source": source, "source_updated_at": state.updated.get(&at)});
            stamp(&mut row);
            out.push("core.prices", row);
            if !same_price {
                state.prices.insert(at.clone(), (raw(price_raw).to_string(), price));
                repriced.extend(world.reserves.get(&at).cloned());
            }
        }
    }
    for asset in repriced {
        let mut of_asset: Vec<f64> = world.reserves.iter().filter(|(_, a)| **a == asset).filter_map(|(r, _)| state.prices.get(r)).map(|(_, p)| *p).collect();
        of_asset.sort_by(f64::total_cmp);
        let n = of_asset.len();
        let median = if n % 2 == 1 { of_asset[n / 2] } else { (of_asset[n / 2 - 1] + of_asset[n / 2]) / 2.0 };
        if state.asset_prices.insert(asset.clone(), median) != Some(median) {
            let mut row = json!({"hub": asset.0, "asset_id": asset.1, "price": median, "sources": n});
            stamp(&mut row);
            out.push("core.asset_prices", row);
        }
    }

    // State rows: one per struct getter read at this block, completed by its sibling views.
    for ((name, target, key), (main, reason)) in &views {
        let ids: Vec<&str> = key.split(':').collect();
        let mut row = match (*name, ids.as_slice()) {
            ("getAsset", [id]) => {
                let (Some(added), Some(owed), Some(fees)) = (view(&views, "getAddedAssets", target, key), view(&views, "getAssetOwed", target, key), view(&views, "getAssetAccruedFees", target, key)) else { continue };
                let decimals = num(&main["decimals"]) as i32;
                let price = id.parse().ok().and_then(|id| state.asset_prices.get(&(target.to_string(), id))).copied().unwrap_or(0.0);
                let (drawn, liquidity, swept) = (scaled(&owed[0], decimals), scaled(&main["liquidity"], decimals), scaled(&main["swept"], decimals));
                let pool = liquidity + drawn + swept;
                json!({
                    "hub": target, "asset_id": id,
                    "liquidity": raw(&main["liquidity"]), "swept": raw(&main["swept"]),
                    "added_shares": raw(&main["addedShares"]), "added_assets": raw(added),
                    "drawn_shares": raw(&main["drawnShares"]), "premium_shares": raw(&main["premiumShares"]),
                    "premium_offset_ray": raw(&main["premiumOffsetRay"]),
                    "drawn_owed": raw(&owed[0]), "premium_owed": raw(&owed[1]),
                    "deficit_ray": raw(&main["deficitRay"]), "realized_fees": raw(&main["realizedFees"]), "accrued_fees": raw(fees),
                    "drawn_index": raw(&main["drawnIndex"]), "drawn_rate": raw(&main["drawnRate"]),
                    "liquidity_fee": raw(&main["liquidityFee"]),
                    "ir_strategy": main["irStrategy"], "reinvestment_controller": main["reinvestmentController"], "fee_receiver": main["feeReceiver"],
                    "usage": if pool > 0.0 { drawn / pool } else { 0.0 },
                    "drawn_rate_apr": scaled(&main["drawnRate"], RAY_DECIMALS),
                    "price": price,
                    "added_usd": scaled(added, decimals) * price,
                    "owed_usd": (drawn + scaled(&owed[1], decimals)) * price,
                    "liquidity_usd": liquidity * price,
                })
            }
            ("getSpoke", [id, spoke]) => {
                let (Some(added), Some(owed)) = (view(&views, "getSpokeAddedAssets", target, key), view(&views, "getSpokeOwed", target, key)) else { continue };
                let asset = id.parse().ok().map(|id| (target.to_string(), id));
                let decimals = asset.as_ref().and_then(|a| world.assets.get(a)).map_or(0, |(_, d)| *d as i32);
                let price = asset.and_then(|a| state.asset_prices.get(&a)).copied().unwrap_or(0.0);
                json!({
                    "hub": target, "asset_id": id, "spoke": spoke,
                    "added_shares": raw(&main["addedShares"]), "added_assets": raw(added),
                    "drawn_shares": raw(&main["drawnShares"]), "premium_shares": raw(&main["premiumShares"]),
                    "premium_offset_ray": raw(&main["premiumOffsetRay"]),
                    "drawn_owed": raw(&owed[0]), "premium_owed": raw(&owed[1]), "deficit_ray": raw(&main["deficitRay"]),
                    "add_cap": raw(&main["addCap"]), "draw_cap": raw(&main["drawCap"]),
                    "risk_premium_threshold": raw(&main["riskPremiumThreshold"]),
                    "active": flag(&main["active"]), "halted": flag(&main["halted"]),
                    "price": price,
                    "added_usd": scaled(added, decimals) * price,
                    "owed_usd": (scaled(&owed[0], decimals) + scaled(&owed[1], decimals)) * price,
                })
            }
            ("getReserve", [id]) => {
                let (Some(config), Some(assets), Some(shares), Some(debt)) = (
                    view(&views, "getReserveConfig", target, key),
                    view(&views, "getReserveSuppliedAssets", target, key),
                    view(&views, "getReserveSuppliedShares", target, key),
                    view(&views, "getReserveDebt", target, key),
                ) else { continue };
                let Ok(reserve) = id.parse::<u32>() else { continue };
                let decimals = num(&main["decimals"]) as i32;
                let config_key = int(main, "dynamicConfigKey").unwrap_or(0);
                let (factor, bonus, fee) = world.dynamic.get(&(target.to_string(), reserve, config_key)).copied().unwrap_or_default();
                let price = state.prices.get(&(target.to_string(), reserve)).map_or(0.0, |(_, p)| *p);
                json!({
                    "spoke": target, "reserve_id": reserve, "hub": main["hub"], "asset_id": raw(&main["assetId"]),
                    "supplied_shares": raw(shares), "supplied_assets": raw(assets),
                    "drawn_debt": raw(&debt[0]), "premium_debt": raw(&debt[1]),
                    "collateral_risk": raw(&config["collateralRisk"]),
                    "paused": flag(&config["paused"]), "frozen": flag(&config["frozen"]),
                    "borrowable": flag(&config["borrowable"]), "receive_shares_enabled": flag(&config["receiveSharesEnabled"]),
                    "dynamic_config_key": config_key, "collateral_factor": factor, "max_liquidation_bonus": bonus, "liquidation_fee": fee,
                    "price": price,
                    "supplied_usd": scaled(assets, decimals) * price,
                    "debt_usd": (scaled(&debt[0], decimals) + scaled(&debt[1], decimals)) * price,
                })
            }
            ("getUserPosition", [id, user]) => {
                let (Some(assets), Some(debt), Some(status)) =
                    (view(&views, "getUserSuppliedAssets", target, key), view(&views, "getUserDebt", target, key), view(&views, "getUserReserveStatus", target, key)) else { continue };
                let Ok(reserve) = id.parse::<u32>() else { continue };
                let at = (target.to_string(), reserve);
                let decimals = world.reserves.get(&at).and_then(|a| world.assets.get(a)).map_or(0, |(_, d)| *d as i32);
                let price = state.prices.get(&at).map_or(0.0, |(_, p)| *p);
                json!({
                    "spoke": target, "user": user, "reserve_id": reserve,
                    "supplied_shares": raw(&main["suppliedShares"]), "supplied_assets": raw(assets),
                    "drawn_shares": raw(&main["drawnShares"]), "premium_shares": raw(&main["premiumShares"]),
                    "premium_offset_ray": raw(&main["premiumOffsetRay"]),
                    "drawn_debt": raw(&debt[0]), "premium_debt": raw(&debt[1]),
                    "is_collateral": flag(&status[0]), "is_borrowing": flag(&status[1]),
                    "dynamic_config_key": raw(&main["dynamicConfigKey"]),
                    "price": price,
                    "supplied_usd": scaled(assets, decimals) * price,
                    "debt_usd": (scaled(&debt[0], decimals) + scaled(&debt[1], decimals)) * price,
                })
            }
            ("getUserAccountData", [user]) => {
                let Some(last) = view(&views, "getUserLastRiskPremium", target, key) else { continue };
                let value = world.oracles.get(*target).and_then(|o| state.oracle_decimals.get(o)).copied().unwrap_or(8) + WAD_DECIMALS;
                let debt = scaled(&main["totalDebtValueRay"], value + RAY_DECIMALS);
                // The contract answers the largest uint256 for an account with no debt.
                let health = if debt > 0.0 { json!(scaled(&main["healthFactor"], WAD_DECIMALS)) } else { json!("inf") };
                json!({
                    "spoke": target, "user": user,
                    "health_factor_raw": raw(&main["healthFactor"]), "risk_premium_raw": raw(&main["riskPremium"]),
                    "last_risk_premium_raw": raw(last), "avg_collateral_factor_raw": raw(&main["avgCollateralFactor"]),
                    "total_collateral_value_raw": raw(&main["totalCollateralValue"]), "total_debt_value_ray": raw(&main["totalDebtValueRay"]),
                    "active_collateral_count": raw(&main["activeCollateralCount"]), "borrow_count": raw(&main["borrowCount"]),
                    "health_factor": health,
                    "risk_premium": scaled(&main["riskPremium"], 4), "last_risk_premium": scaled(last, 4),
                    "collateral_usd": scaled(&main["totalCollateralValue"], value), "debt_usd": debt,
                })
            }
            _ => continue,
        };
        row["reason"] = json!(reason);
        stamp(&mut row);
        let table = match *name {
            "getAsset" => "core.hub_asset_state",
            "getSpoke" => "core.hub_spoke_state",
            "getReserve" => "core.reserve_state",
            "getUserPosition" => "core.user_reserve_state",
            _ => "core.user_account_state",
        };
        out.push(table, row);
    }

    for (log, abi_kind, name, args) in &events {
        let contract = text(log, "address");
        let kind = match () {
            _ if world.hubs.contains(contract) => "hub",
            _ if world.oracles.contains_key(contract) => "spoke",
            _ if world.vaults.contains_key(contract) => "tokenization",
            _ if world.oracles.values().any(|o| o == contract) => "oracle",
            _ => abi_kind,
        };
        let reserve = ["reserveId", "debtReserveId"].iter().find_map(|k| int(args, k)).filter(|_| kind == "spoke");
        let (hub, asset, spoke) = match kind {
            "hub" => (contract.to_string(), int(args, "assetId"), ["spoke", "callerSpoke"].iter().map(|k| text(args, k)).find(|s| !s.is_empty()).unwrap_or("")),
            "spoke" => match reserve.and_then(|r| world.reserves.get(&(contract.to_string(), r))) {
                Some((hub, asset)) => (hub.clone(), Some(*asset), contract),
                None => (String::new(), None, contract),
            },
            "tokenization" => world.vaults.get(contract).map_or((String::new(), None, contract), |(hub, asset)| (hub.clone(), Some(*asset), contract)),
            "ir_strategy" => (text(args, "hub").to_string(), int(args, "assetId"), ""),
            _ => (String::new(), None, ""),
        };
        let first = |keys: &[&str]| keys.iter().map(|k| text(args, k)).find(|s| !s.is_empty()).unwrap_or("");
        let mut amount: u128 = first(&["amount", "drawnAmount", "suppliedAmount", "withdrawnAmount", "totalAmountRepaid", "debtAmountRestored", "assets"]).parse().unwrap_or(0);
        if *name == "Restore" {
            amount += text(args, "premiumAmount").parse::<u128>().unwrap_or(0);
        }
        // A vault share Transfer moves `value` shares; an Approval's `value` is an allowance, not a move.
        let transfer: &[&str] = if *name == "Transfer" { &["value"] } else { &[] };
        let shares = first(&[&["shares", "drawnShares", "suppliedShares", "withdrawnShares", "drawnSharesLiquidated"], transfer].concat());
        let (token, decimals) = asset.and_then(|a| world.assets.get(&(hub.clone(), a))).cloned().unwrap_or_default();
        let price = match reserve {
            Some(r) => state.prices.get(&(contract.to_string(), r)).map(|(_, p)| *p),
            None => asset.and_then(|a| state.asset_prices.get(&(hub.clone(), a))).copied(),
        }
        .unwrap_or(0.0);
        let mut row = json!({
            "log_index": log["log_index"], "tx_hash": log["tx_hash"], "contract": contract, "kind": kind, "name": name,
            "hub": hub, "spoke": spoke, "asset_id": asset, "reserve_id": reserve,
            "user": first(&["user", "owner", "from"]), "caller": first(&["caller", "liquidator", "sender"]),
            "amount": amount.to_string(), "shares": if shares.is_empty() { "0" } else { shares },
            "token": token, "price": price, "amount_usd": amount as f64 / 10f64.powi(decimals as i32) * price,
            "args": args.to_string(),
        });
        stamp(&mut row);
        out.push("core.events", row);
    }
}

impl ClickHouse {
    /// Stored answers of `[from, to]` in block order, each once. Reverted calls are left out.
    /// `only` is an SQL condition that narrows them, or empty.
    async fn calls(&self, chain_id: u32, from: u64, to: u64, only: &str) -> Result<Vec<Value>> {
        let only = if only.is_empty() { String::new() } else { format!("AND ({only})") };
        let sql = format!(
            "SELECT block_number, toUnixTimestamp(block_time) AS t, target, selector, args, result, reason \
             FROM raw.calls WHERE chain_id = {chain_id} AND block_number BETWEEN {from} AND {to} AND ok = 1 {only} \
             ORDER BY block_number LIMIT 1 BY block_number, target, selector, args FORMAT JSONEachRow"
        );
        self.query(&sql, String::new()).await?.lines().map(|l| Ok(serde_json::from_str(l)?)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aave_watch_protocol::abi;
    use aave_watch_protocol::codec::{DynSolValue, U256, address, uints};

    const HUB: &str = "0xcca852bc40e560adc3b1cc58ca5b55638ce826c9";
    const SPOKE: &str = "0x94e7a5dcbe816e498b89ab752661904e2f56c485";
    const ORACLE: &str = "0x6b3800e96bb4aa38a879b2c143b6e2f5f2687b77";
    const USER: &str = "0x1111111111111111111111111111111111111111";
    const TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

    /// `n * 10^exp` as one 32-byte ABI word.
    fn word(n: u64, exp: u32) -> String {
        format!("{:064x}", U256::from(n) * U256::from(10).pow(U256::from(exp)))
    }

    fn topic(address: &str) -> String {
        format!("0x{:0>64}", &address[2..])
    }

    fn log(index: u32, address: &str, kind: &str, event: &str, topics: [String; 3], data: String) -> Value {
        json!({"block_number": 100, "t": 1_800_000_000u64, "log_index": index, "tx_hash": "0xaa", "address": address,
               "topic0": abi::topic0(kind, event).unwrap(), "topic1": topics[0], "topic2": topics[1], "topic3": topics[2], "data": data})
    }

    fn call(codec: &Codec, target: &str, view: &str, args: &[DynSolValue], result: String) -> Value {
        let data = codec.call(view, args);
        json!({"block_number": 100, "t": 1_800_000_000u64, "target": target, "selector": &data[..10],
               "args": format!("0x{}", &data[10..]), "result": format!("0x{result}"), "reason": "event"})
    }

    #[test]
    fn a_block_becomes_priced_events_and_state_rows() {
        let codec = Codec::new();
        let mut world = World { hubs: [HUB.to_string()].into(), ..World::default() };
        let mut state = State::default();
        let none = String::new;
        let id = |n: u64| format!("0x{}", word(n, 0));
        let logs = [
            log(0, HUB, "hub", "AddAsset", [id(3), topic(TOKEN), none()], id(6)),
            log(1, SPOKE, "spoke", "SetSpokeImmutables", [topic(ORACLE), none(), none()], id(16)),
            log(2, SPOKE, "spoke", "AddReserve", [id(0), id(3), topic(HUB)], "0x".into()),
            // Borrow of 250 USDC (6 decimals) from reserve 0
            log(3, SPOKE, "spoke", "Borrow", [id(0), topic(USER), topic(USER)], format!("0x{}{}", word(240, 6), word(250, 6))),
        ];
        let user = &[address(USER)];
        let calls = [
            call(&codec, ORACLE, "decimals", &[], word(8, 0)),
            // one reserve, priced at 1.0002 USD
            call(&codec, ORACLE, "getReservesPrices", &[uints(&[0])], [word(0x20, 0), word(1, 0), word(100_020_000, 0)].concat()),
            // risk premium 5%, collateral factor 0.65, health factor 2.6, collateral 1000 USD, debt 250.05 USD
            call(&codec, SPOKE, "getUserAccountData", user,
                 [word(500, 0), word(65, 16), word(26, 17), word(1000, 26), word(25_005, 24 + 27), word(1, 0), word(1, 0)].concat()),
            call(&codec, SPOKE, "getUserLastRiskPremium", user, word(400, 0)),
        ];
        let mut out = Rows::default();
        let (logs, calls): (Vec<&Value>, Vec<&Value>) = (logs.iter().collect(), calls.iter().collect());
        process_block(&codec, &mut world, &mut state, 1, 100, &logs, &calls, &mut out);

        assert_eq!(out.0["core.prices"][0]["price"], 1.0002);
        assert_eq!(out.0["core.asset_prices"][0]["price"], 1.0002);
        assert_eq!(out.0["meta.reserves"][0]["token"], TOKEN);

        let borrow = out.0["core.events"].iter().find(|e| e["name"] == "Borrow").unwrap();
        assert_eq!(borrow["kind"], "spoke");
        assert_eq!(borrow["hub"], HUB);
        assert_eq!(borrow["asset_id"], 3);
        assert_eq!(borrow["user"], USER);
        assert_eq!(borrow["amount"], "250000000");
        assert_eq!(borrow["shares"], "240000000");
        assert!((borrow["amount_usd"].as_f64().unwrap() - 250.05).abs() < 1e-9);
        assert_eq!(borrow["block_number"], 100);

        let account = &out.0["core.user_account_state"][0];
        assert_eq!(account["health_factor"], 2.6);
        assert!((account["collateral_usd"].as_f64().unwrap() - 1000.0).abs() < 1e-9);
        assert!((account["debt_usd"].as_f64().unwrap() - 250.05).abs() < 1e-9);
        assert_eq!(account["risk_premium"], 0.05);
        assert_eq!(account["last_risk_premium"], 0.04);

        // the same price again writes no second price row
        let mut again = Rows::default();
        process_block(&codec, &mut world, &mut state, 1, 101, &[], &calls[1..2], &mut again);
        assert!(!again.0.contains_key("core.prices"));
        // unless its source reports a new update time: that is a row, with the source named
        let feed = log(0, SPOKE, "spoke", "UpdateReservePriceSource", [id(0), topic(TOKEN), none()], "0x".into());
        let round = call(&codec, TOKEN, "latestRoundData", &[], [word(7, 0), word(100_020_000, 0), word(0, 0), word(1_799_999_000, 0), word(7, 0)].concat());
        process_block(&codec, &mut world, &mut state, 1, 101, &[&feed], &[calls[1], &round], &mut again);
        assert_eq!(again.0["core.prices"][0]["source"], TOKEN);
        assert_eq!(again.0["core.prices"][0]["source_updated_at"], 1_799_999_000u64);
        assert!(!again.0.contains_key("core.asset_prices")); // the price itself did not move

        // no debt: the health factor is infinite, not the largest uint256 divided by 1e18
        let max = format!("{:064x}", U256::MAX);
        let idle = call(&codec, SPOKE, "getUserAccountData", user, [word(0, 0), word(0, 0), max, word(0, 0), word(0, 0), word(0, 0), word(0, 0)].concat());
        let last = call(&codec, SPOKE, "getUserLastRiskPremium", user, word(0, 0));
        let mut rows = Rows::default();
        process_block(&codec, &mut world, &mut state, 1, 102, &[], &[&idle, &last], &mut rows);
        assert_eq!(rows.0["core.user_account_state"][0]["health_factor"], "inf");
    }
}
