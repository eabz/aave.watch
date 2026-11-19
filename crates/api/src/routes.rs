//! The endpoints of docs/api.md. Each list endpoint is one SELECT with named parameters.

use crate::{App, stream};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

type Q = Query<HashMap<String, String>>;
type S = State<Arc<App>>;

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/chains", get(chains))
        .route("/v1/hubs", get(hubs))
        .route("/v1/spokes", get(spokes))
        .route("/v1/summary", get(summary))
        .route("/v1/hub-assets", get(hub_assets))
        .route("/v1/hub-spokes", get(hub_spokes))
        .route("/v1/reserves", get(reserves))
        .route("/v1/positions", get(positions))
        .route("/v1/account", get(account))
        .route("/v1/holders", get(holders))
        .route("/v1/events", get(events))
        .route("/v1/liquidations", get(liquidations))
        .route("/v1/history/hub-asset", get(history_hub_asset))
        .route("/v1/history/totals", get(history_totals))
        .route("/v1/history/v3-totals", get(history_v3_totals))
        .route("/v1/history/account", get(history_account))
        .route("/v1/stream", get(sse))
        .fallback(|| async { fail(StatusCode::NOT_FOUND, "No such endpoint. See docs/api.md.") })
        .with_state(app)
}

fn fail(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(json!({ "error": msg.into() }))).into_response()
}

fn ok(body: Value, max_age: u32) -> Response {
    ([(header::CACHE_CONTROL, format!("public, max-age={max_age}"))], Json(body)).into_response()
}

// ---- parameters: validated here, at the trust boundary --------------------------------------

struct Params<'a>(&'a HashMap<String, String>);

impl Params<'_> {
    fn chain(&self) -> Result<String, Response> {
        match self.0.get("chain").map(String::as_str) {
            None | Some("") => Ok("0".into()),
            Some(c) => c.parse::<u32>().map(|c| c.to_string()).map_err(|_| fail(StatusCode::BAD_REQUEST, "chain must be a chain id, like 1.")),
        }
    }
    /// An optional `0x` address, lowercased; empty when absent.
    fn address(&self, name: &str) -> Result<String, Response> {
        match self.0.get(name).map(|v| v.to_ascii_lowercase()) {
            None => Ok(String::new()),
            Some(v) if v.is_empty() => Ok(v),
            Some(v) if v.len() == 42 && v.starts_with("0x") && v[2..].bytes().all(|b| b.is_ascii_hexdigit()) => Ok(v),
            Some(_) => Err(fail(StatusCode::BAD_REQUEST, format!("{name} must be a 0x address of 40 hex digits."))),
        }
    }
    fn word(&self, name: &str) -> Result<String, Response> {
        match self.0.get(name) {
            None => Ok(String::new()),
            Some(v) if v.len() <= 64 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') => Ok(v.clone()),
            Some(_) => Err(fail(StatusCode::BAD_REQUEST, format!("{name} must be letters and digits."))),
        }
    }
    /// An optional token symbol (`USDC`, `WETH.e`, `PT-sUSDE-7MAY2026`), lowercased; empty when absent.
    fn symbol(&self) -> Result<String, Response> {
        match self.0.get("symbol") {
            None => Ok(String::new()),
            Some(v) if v.len() <= 64 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) => Ok(v.to_ascii_lowercase()),
            Some(_) => Err(fail(StatusCode::BAD_REQUEST, "symbol must be a token symbol, like USDC.")),
        }
    }
    fn uint(&self, name: &str, default: u64, max: u64) -> Result<String, Response> {
        match self.0.get(name) {
            None => Ok(default.to_string()),
            Some(v) => v.parse::<u64>().map(|n| n.min(max).to_string()).map_err(|_| fail(StatusCode::BAD_REQUEST, format!("{name} must be a whole number."))),
        }
    }
    fn float(&self, name: &str, default: f64) -> Result<f64, Response> {
        match self.0.get(name) {
            None => Ok(default),
            Some(v) => v.parse::<f64>().ok().filter(|f| f.is_finite()).ok_or_else(|| fail(StatusCode::BAD_REQUEST, format!("{name} must be a number, like 1.1."))),
        }
    }
    /// `step`: one item per hour (the default) or per day.
    fn step(&self) -> Result<String, Response> {
        match self.0.get("step").map(String::as_str) {
            None | Some("hour") => Ok("hour".into()),
            Some("day") => Ok("day".into()),
            Some(_) => Err(fail(StatusCode::BAD_REQUEST, "step must be hour or day.")),
        }
    }
    fn required(&self, name: &str, value: String) -> Result<String, Response> {
        if value.is_empty() || value == "0" { Err(fail(StatusCode::BAD_REQUEST, format!("{name} is required."))) } else { Ok(value) }
    }
}

async fn list(app: &App, sql: &str, params: &[(&str, String)], max_age: u32) -> Response {
    match app.ch.query(sql, params).await {
        Ok(rows) => ok(json!({ "data": rows }), max_age),
        Err(e) => fail(StatusCode::BAD_GATEWAY, e),
    }
}

macro_rules! p {
    ($e:expr) => { match $e { Ok(v) => v, Err(r) => return r } };
}

// Shared SQL fragments. `{x:T} = default OR col = {x:T}` makes a filter optional.
const BY_CHAIN: &str = "({chain:UInt32} = 0 OR chain_id = {chain:UInt32})";
const HIDE: &str = "EXCEPT (inserted_at, parser_version)";

// ---- registry -------------------------------------------------------------------------------

async fn chains(State(app): S) -> Response {
    list(&app, "SELECT chain_id, name, block_time_ms, settle_tag, explorer_url FROM meta.chains FINAL ORDER BY chain_id", &[], 300).await
}

async fn hubs(State(app): S, Query(q): Q) -> Response {
    let chain = p!(Params(&q).chain());
    let sql = format!("SELECT chain_id, address, name, first_block FROM meta.hubs FINAL WHERE {BY_CHAIN} ORDER BY chain_id, first_block");
    list(&app, &sql, &[("chain", chain)], 300).await
}

async fn spokes(State(app): S, Query(q): Q) -> Response {
    let (chain, kind) = (p!(Params(&q).chain()), p!(Params(&q).word("kind")));
    let sql = format!("SELECT chain_id, address, name, kind, oracle, first_block FROM meta.spokes FINAL \
        WHERE {BY_CHAIN} AND ({{kind:String}} = '' OR kind = {{kind:String}}) ORDER BY chain_id, kind, name, address");
    list(&app, &sql, &[("chain", chain), ("kind", kind)], 300).await
}

// ---- latest state ---------------------------------------------------------------------------

async fn summary(State(app): S, Query(q): Q) -> Response {
    let chain = p!(Params(&q).chain());
    let sql = format!("SELECT chain_id, supplied_usd, borrowed_usd, liquidity_usd, positions_below_1_1, debt_below_1_1, liquidatable, block_number, block_time FROM \
        (SELECT chain_id, sum(added_usd) AS supplied_usd, sum(owed_usd) AS borrowed_usd, sum(liquidity_usd) AS liquidity_usd, \
                max(block_number) AS block_number, max(block_time) AS block_time \
         FROM core.hub_asset_latest FINAL WHERE {BY_CHAIN} GROUP BY chain_id) AS h \
        FULL JOIN \
        (SELECT chain_id, countIf(health_factor < 1.1) AS positions_below_1_1, sumIf(debt_usd, health_factor < 1.1) AS debt_below_1_1, \
                countIf(health_factor < 1) AS liquidatable \
         FROM core.user_account_latest FINAL WHERE {BY_CHAIN} AND debt_usd > 0 GROUP BY chain_id) AS u USING (chain_id) \
        ORDER BY chain_id");
    list(&app, &sql, &[("chain", chain)], 15).await
}

async fn hub_assets(State(app): S, Query(q): Q) -> Response {
    let (chain, hub) = (p!(Params(&q).chain()), p!(Params(&q).address("hub")));
    let sql = format!("SELECT s.* {HIDE}, dictGetOrDefault('meta.hubs_dict', 'name', (s.chain_id, toString(s.hub)), '') AS hub_name, \
               a.token AS token, a.decimals AS decimals, \
               dictGetOrDefault('meta.tokens_dict', 'symbol', (s.chain_id, a.token), '') AS symbol, \
               y.supply_apy_1d AS supply_apy_1d, y.supply_apy_7d AS supply_apy_7d \
        FROM (SELECT * FROM core.hub_asset_latest FINAL WHERE {BY_CHAIN} AND ({{hub:String}} = '' OR hub = {{hub:String}})) AS s \
        LEFT JOIN (SELECT chain_id, hub, asset_id, token, decimals FROM meta.assets FINAL) AS a \
          ON a.chain_id = s.chain_id AND a.hub = s.hub AND a.asset_id = s.asset_id \
        LEFT JOIN (SELECT chain_id, hub, asset_id, supply_apy_1d, supply_apy_7d FROM core.hub_asset_apy) AS y \
          ON y.chain_id = s.chain_id AND y.hub = s.hub AND y.asset_id = s.asset_id \
        ORDER BY s.chain_id, s.hub, s.asset_id");
    list(&app, &sql, &[("chain", chain), ("hub", hub)], 15).await
}

async fn hub_spokes(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let (chain, hub, spoke) = (p!(pr.chain()), p!(pr.address("hub")), p!(pr.address("spoke")));
    let asset = q.get("asset_id").map(|a| a.parse::<u32>().map(|a| a as i64)).transpose();
    let asset = p!(asset.map_err(|_| fail(StatusCode::BAD_REQUEST, "asset_id must be a whole number."))).unwrap_or(-1);
    let sql = format!("SELECT s.* {HIDE}, dictGetOrDefault('meta.spokes_dict', 'name', (s.chain_id, toString(s.spoke)), '') AS spoke_name, \
               dictGetOrDefault('meta.spokes_dict', 'kind', (s.chain_id, toString(s.spoke)), '') AS spoke_kind, \
               a.decimals AS decimals, dictGetOrDefault('meta.tokens_dict', 'symbol', (s.chain_id, a.token), '') AS symbol \
        FROM (SELECT * FROM core.hub_spoke_latest FINAL WHERE {BY_CHAIN} \
                AND ({{hub:String}} = '' OR hub = {{hub:String}}) AND ({{spoke:String}} = '' OR spoke = {{spoke:String}}) \
                AND ({{asset:Int64}} < 0 OR asset_id = {{asset:Int64}})) AS s \
        LEFT JOIN (SELECT chain_id, hub, asset_id, token, decimals FROM meta.assets FINAL) AS a \
          ON a.chain_id = s.chain_id AND a.hub = s.hub AND a.asset_id = s.asset_id \
        ORDER BY s.chain_id, s.hub, s.asset_id, s.added_usd DESC");
    list(&app, &sql, &[("chain", chain), ("hub", hub), ("spoke", spoke), ("asset", asset.to_string())], 15).await
}

async fn reserves(State(app): S, Query(q): Q) -> Response {
    let (chain, spoke) = (p!(Params(&q).chain()), p!(Params(&q).address("spoke")));
    let sql = format!("SELECT s.* {HIDE}, r.token AS token, r.decimals AS decimals, \
               dictGetOrDefault('meta.tokens_dict', 'symbol', (s.chain_id, r.token), '') AS symbol \
        FROM (SELECT * FROM core.reserve_latest FINAL WHERE {BY_CHAIN} AND ({{spoke:String}} = '' OR spoke = {{spoke:String}})) AS s \
        LEFT JOIN (SELECT chain_id, spoke, reserve_id, token, decimals FROM meta.reserves FINAL) AS r \
          ON r.chain_id = s.chain_id AND r.spoke = s.spoke AND r.reserve_id = s.reserve_id \
        ORDER BY s.chain_id, s.spoke, s.reserve_id");
    list(&app, &sql, &[("chain", chain), ("spoke", spoke)], 15).await
}

const ACCOUNT_COLUMNS: &str = "* EXCEPT (inserted_at, parser_version), \
    dictGetOrDefault('meta.spokes_dict', 'name', (chain_id, toString(spoke)), '') AS spoke_name";

async fn positions(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let (chain, spoke) = (p!(pr.chain()), p!(pr.address("spoke")));
    let (max_hf, limit) = (p!(pr.float("max_hf", 1.5)), p!(pr.uint("limit", 100, 1000)));

    // The tip first: block-fresh, straight from the sorted sets the indexer keeps.
    let listed = app.ch.query(
        &format!("SELECT chain_id, address, name FROM meta.spokes FINAL WHERE kind = 'spoke' AND {BY_CHAIN} AND ({{spoke:String}} = '' OR address = {{spoke:String}})"),
        &[("chain", chain.clone()), ("spoke", spoke.clone())]).await;
    if let Ok(listed) = listed {
        let spokes: Vec<(u32, String, String)> = listed.iter().filter_map(|r| Some((
            r["chain_id"].as_u64()? as u32, r["address"].as_str()?.to_string(), r["name"].as_str().unwrap_or("").to_string()))).collect();
        if let Ok(Some(rows)) = app.tip.positions(&spokes, max_hf, limit.parse().unwrap_or(100)).await {
            return ok(json!({ "data": rows, "source": "tip" }), 5);
        }
    }
    // Otherwise the last settled reading of each account.
    let sql = format!("SELECT {ACCOUNT_COLUMNS} FROM core.user_account_latest FINAL \
        WHERE {BY_CHAIN} AND ({{spoke:String}} = '' OR spoke = {{spoke:String}}) AND debt_usd > 0 AND health_factor < {{max_hf:Float64}} \
        ORDER BY health_factor LIMIT {{limit:UInt32}}");
    match app.ch.query(&sql, &[("chain", chain), ("spoke", spoke), ("max_hf", max_hf.to_string()), ("limit", limit)]).await {
        Ok(rows) => ok(json!({ "data": rows, "source": "settled" }), 5),
        Err(e) => fail(StatusCode::BAD_GATEWAY, e),
    }
}

async fn account(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let chain = p!(pr.chain().and_then(|c| pr.required("chain", c)));
    let user = p!(pr.address("user").and_then(|u| pr.required("user", u)));
    let params = [("chain", chain), ("user", user)];
    let accounts = app.ch.query(&format!("SELECT {ACCOUNT_COLUMNS} FROM core.user_account_latest FINAL \
        WHERE chain_id = {{chain:UInt32}} AND user = {{user:String}} ORDER BY spoke"), &params).await;
    let reserves = app.ch.query("SELECT s.* EXCEPT (inserted_at, parser_version), r.decimals AS decimals, \
               dictGetOrDefault('meta.tokens_dict', 'symbol', (s.chain_id, r.token), '') AS symbol \
        FROM (SELECT * FROM core.user_reserve_latest FINAL WHERE chain_id = {chain:UInt32} AND user = {user:String}) AS s \
        LEFT JOIN (SELECT chain_id, spoke, reserve_id, token, decimals FROM meta.reserves FINAL) AS r \
          ON r.chain_id = s.chain_id AND r.spoke = s.spoke AND r.reserve_id = s.reserve_id \
        ORDER BY s.spoke, s.reserve_id", &params).await;
    match (accounts, reserves) {
        (Ok(mut accounts), Ok(reserves)) => {
            for a in accounts.iter_mut() {
                let mine: Vec<Value> = reserves.iter().filter(|r| r["spoke"] == a["spoke"]).cloned().collect();
                a["reserves"] = Value::Array(mine);
            }
            ok(json!({ "data": accounts }), 15)
        }
        (Err(e), _) | (_, Err(e)) => fail(StatusCode::BAD_GATEWAY, e),
    }
}

/// The wallets that hold the most of one token, every market and chain added up. `by=debt` ranks
/// by what they have borrowed of it instead.
async fn holders(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let symbol = p!(pr.symbol().and_then(|s| pr.required("symbol", s)));
    let (chain, limit) = (p!(pr.chain()), p!(pr.uint("limit", 20, 200)));
    let by = match q.get("by").map(String::as_str) {
        None | Some("supplied") => "supplied_usd",
        Some("debt") => "debt_usd",
        Some(_) => return fail(StatusCode::BAD_REQUEST, "by must be supplied or debt."),
    };
    let sql = format!("SELECT user, sum(s.supplied_usd) AS supplied_usd, sum(s.debt_usd) AS debt_usd, uniqExact(s.chain_id, s.spoke) AS markets, \
               groupUniqArray(s.chain_id) AS chains, max(s.block_number) AS block_number, max(s.block_time) AS block_time \
        FROM (SELECT * FROM core.user_reserve_latest FINAL WHERE {BY_CHAIN}) AS s \
        INNER JOIN (SELECT chain_id, spoke, reserve_id FROM meta.reserves FINAL \
                    WHERE lower(dictGetOrDefault('meta.tokens_dict', 'symbol', (chain_id, token), '')) = {{symbol:String}}) AS r \
          ON r.chain_id = s.chain_id AND r.spoke = s.spoke AND r.reserve_id = s.reserve_id \
        GROUP BY user HAVING {by} > 0 ORDER BY {by} DESC LIMIT {{limit:UInt32}}");
    list(&app, &sql, &[("chain", chain), ("symbol", symbol), ("limit", limit)], 60).await
}

// ---- history --------------------------------------------------------------------------------

async fn events(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let (chain, spoke, hub, user) = (p!(pr.chain()), p!(pr.address("spoke")), p!(pr.address("hub")), p!(pr.address("user")));
    let (name, before, limit, symbol) = (p!(pr.word("name")), p!(pr.uint("before", 0, u64::MAX)), p!(pr.uint("limit", 100, 1000)), p!(pr.symbol()));
    // LIMIT 1 BY drops a row written twice without paying for FINAL on the big table.
    let sql = format!("SELECT * {HIDE}, dictGetOrDefault('meta.tokens_dict', 'symbol', (chain_id, toString(token)), '') AS symbol, \
               dictGetOrDefault('meta.spokes_dict', 'name', (chain_id, toString(spoke)), '') AS spoke_name \
        FROM core.events WHERE {BY_CHAIN} \
          AND ({{spoke:String}} = '' OR spoke = {{spoke:String}}) AND ({{hub:String}} = '' OR hub = {{hub:String}}) \
          AND ({{user:String}} = '' OR user = {{user:String}}) AND ({{name:String}} = '' OR name = {{name:String}}) \
          AND ({{before:UInt64}} = 0 OR block_number < {{before:UInt64}}) \
          AND ({{symbol:String}} = '' OR lower(dictGetOrDefault('meta.tokens_dict', 'symbol', (chain_id, toString(token)), '')) = {{symbol:String}}) \
        ORDER BY block_time DESC, block_number DESC, log_index DESC \
        LIMIT 1 BY chain_id, block_number, log_index LIMIT {{limit:UInt32}}");
    let params = [("chain", chain), ("spoke", spoke), ("hub", hub), ("user", user), ("name", name), ("before", before), ("limit", limit), ("symbol", symbol)];
    match app.ch.query(&sql, &params).await {
        Ok(mut rows) => {
            for r in rows.iter_mut() {
                // args is stored as JSON text; hand it over as an object
                if let Some(parsed) = r["args"].as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()) { r["args"] = parsed; }
            }
            ok(json!({ "data": rows }), 15)
        }
        Err(e) => fail(StatusCode::BAD_GATEWAY, e),
    }
}

async fn liquidations(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let (chain, spoke, user, liquidator) = (p!(pr.chain()), p!(pr.address("spoke")), p!(pr.address("user")), p!(pr.address("liquidator")));
    let (before, limit) = (p!(pr.uint("before", 0, u64::MAX)), p!(pr.uint("limit", 100, 1000)));
    let sql = format!("SELECT *, dictGetOrDefault('meta.tokens_dict', 'symbol', (chain_id, debt_token), '') AS debt_symbol, \
               dictGetOrDefault('meta.tokens_dict', 'symbol', (chain_id, collateral_token), '') AS collateral_symbol, \
               dictGetOrDefault('meta.spokes_dict', 'name', (chain_id, spoke), '') AS spoke_name \
        FROM core.liquidations WHERE {BY_CHAIN} \
          AND ({{spoke:String}} = '' OR spoke = {{spoke:String}}) AND ({{user:String}} = '' OR user = {{user:String}}) \
          AND ({{liquidator:String}} = '' OR liquidator = {{liquidator:String}}) \
          AND ({{before:UInt64}} = 0 OR block_number < {{before:UInt64}}) \
        ORDER BY block_time DESC, block_number DESC, log_index DESC LIMIT {{limit:UInt32}}");
    list(&app, &sql, &[("chain", chain), ("spoke", spoke), ("user", user), ("liquidator", liquidator), ("before", before), ("limit", limit)], 15).await
}

async fn history_hub_asset(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let chain = p!(pr.chain().and_then(|c| pr.required("chain", c)));
    let hub = p!(pr.address("hub").and_then(|h| pr.required("hub", h)));
    let asset = p!(pr.uint("asset_id", u64::MAX, u32::MAX as u64));
    if !q.contains_key("asset_id") { return fail(StatusCode::BAD_REQUEST, "asset_id is required."); }
    let hours = p!(pr.uint("hours", 720, 24 * 366 * 5));
    list(&app, "SELECT * EXCEPT (refreshed_at) FROM core.hub_asset_hourly FINAL \
        WHERE chain_id = {chain:UInt32} AND hub = {hub:String} AND asset_id = {asset:UInt32} AND hour >= now() - toIntervalHour({hours:UInt32}) \
        ORDER BY hour", &[("chain", chain), ("hub", hub), ("asset", asset), ("hours", hours)], 300).await
}

/// Hourly totals per chain from per-asset hourly rows. An asset with no reading in an hour keeps
/// its last one (WITH FILL), so a quiet asset does not dip the total. `rows` selects
/// `chain_id, <key columns>, hour, supplied_usd, borrowed_usd, liquidity_usd, block_number`.
async fn totals(app: &App, q: &HashMap<String, String>, rows: &str, key: &str, extra: &str) -> Response {
    let pr = Params(q);
    let chain = p!(pr.chain());
    let hours = p!(pr.uint("hours", 720, 24 * 366 * 5));
    let step = p!(pr.step());
    let hub = p!(pr.address("hub"));
    let sql = format!("SELECT chain_id, {extra} hour, sum(supplied_usd) AS supplied_usd, sum(borrowed_usd) AS borrowed_usd, sum(liquidity_usd) AS liquidity_usd, \
               max(block_number) AS block_number, hour AS block_time FROM ( \
          {rows} ORDER BY chain_id, {key}, hour WITH FILL TO toStartOfHour(now()) + toIntervalHour(1) STEP toIntervalHour(1) \
          INTERPOLATE (supplied_usd AS supplied_usd, borrowed_usd AS borrowed_usd, liquidity_usd AS liquidity_usd, block_number AS block_number)) \
        WHERE hour >= now() - toIntervalHour({{hours:UInt32}}) AND ({{step:String}} = 'hour' OR toHour(hour) = 0 OR hour = toStartOfHour(now())) \
        GROUP BY chain_id, hour ORDER BY chain_id, hour");
    list(app, &sql, &[("chain", chain), ("hours", hours), ("step", step), ("hub", hub)], 300).await
}

/// One wallet's collateral, debt and health factor over time, per Spoke. An hour with no reading
/// keeps the last one (WITH FILL, per Spoke), from the wallet's first reading to its newest; the
/// stamp stays that of the reading carried. The whole history of the wallet is filled before
/// `hours` cuts it, so the first hour asked for is carried from before the window.
async fn history_account(State(app): S, Query(q): Q) -> Response {
    let pr = Params(&q);
    let chain = p!(pr.chain().and_then(|c| pr.required("chain", c)));
    let user = p!(pr.address("user").and_then(|u| pr.required("user", u)));
    let (spoke, hours, step) = (p!(pr.address("spoke")), p!(pr.uint("hours", 720, 24 * 366 * 5)), p!(pr.step()));
    let sql = "SELECT chain_id, spoke, dictGetOrDefault('meta.spokes_dict', 'name', (chain_id, spoke), '') AS spoke_name, hour, \
               if(debt_usd > 0, health_factor, NULL) AS health_factor, collateral_usd, debt_usd, risk_premium, block_number, block_time FROM ( \
          SELECT *, max(hour) OVER (PARTITION BY spoke) AS newest FROM ( \
            SELECT chain_id, toString(spoke) AS spoke, hour, health_factor, collateral_usd, debt_usd, risk_premium, block_number, block_time \
            FROM core.user_account_hourly FINAL \
            WHERE chain_id = {chain:UInt32} AND user = {user:String} AND ({spoke:String} = '' OR spoke = {spoke:String}) \
            ORDER BY chain_id, spoke, hour WITH FILL STEP toIntervalHour(1) \
            INTERPOLATE (health_factor AS health_factor, collateral_usd AS collateral_usd, debt_usd AS debt_usd, \
                         risk_premium AS risk_premium, block_number AS block_number, block_time AS block_time))) \
        WHERE hour >= now() - toIntervalHour({hours:UInt32}) AND ({step:String} = 'hour' OR toHour(hour) = 0 OR hour = newest) \
        ORDER BY spoke, hour";
    list(&app, sql, &[("chain", chain), ("user", user), ("spoke", spoke), ("hours", hours), ("step", step)], 300).await
}

async fn history_totals(State(app): S, Query(q): Q) -> Response {
    let rows = format!("SELECT chain_id, hub, asset_id, hour, added_usd AS supplied_usd, owed_usd AS borrowed_usd, liquidity_usd, block_number \
        FROM core.hub_asset_hourly FINAL WHERE {BY_CHAIN} AND ({{hub:String}} = '' OR hub = {{hub:String}})");
    totals(&app, &q, &rows, "hub, asset_id", "").await
}

/// The same totals for Aave v3, every market of a chain added up, with the chain's name (v3 runs
/// on chains `meta.chains` does not list).
async fn history_v3_totals(State(app): S, Query(q): Q) -> Response {
    let mut names: Vec<(u32, String)> = aave_watch_protocol::registry::v3_markets().into_iter().map(|m| (m.chain_id, m.chain)).collect();
    names.dedup();
    let (ids, names): (Vec<String>, Vec<String>) = names.into_iter().map(|(id, name)| (id.to_string(), format!("'{name}'"))).unzip();
    // every reserve of a market is read at the same block, so a market is summed before the fill
    let rows = format!("SELECT chain_id, market, toStartOfHour(t) AS hour, argMax(s, n) AS supplied_usd, argMax(b, n) AS borrowed_usd, \
               argMax(s - b, n) AS liquidity_usd, max(n) AS block_number FROM ( \
          SELECT chain_id, market, block_number AS n, any(block_time) AS t, sum(supplied_usd) AS s, sum(borrowed_usd) AS b \
          FROM core.v3_reserve_state WHERE {BY_CHAIN} GROUP BY chain_id, market, n) GROUP BY chain_id, market, hour");
    let extra = format!("transform(chain_id, [{}], [{}], '') AS chain,", ids.join(","), names.join(","));
    totals(&app, &q, &rows, "market", &extra).await
}

// ---- health and stream ----------------------------------------------------------------------

async fn health(State(app): S) -> Response {
    let ch = app.ch.query("SELECT 1 AS ok", &[]).await;
    let heads = app.tip.heads(&app.chains).await;
    let body = json!({
        "clickhouse": ch.as_ref().err().cloned().unwrap_or_else(|| "ok".into()),
        "dragonfly": heads.as_ref().err().cloned().unwrap_or_else(|| "ok".into()),
        "heads": heads.unwrap_or_default(),
    });
    let status = if ch.is_ok() { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

fn sse_event(item: &stream::Item) -> Event {
    Event::default().id(format!("{}/{}", item.stream, item.id)).event(item.topic.clone()).data(item.json.clone())
}

async fn sse(State(app): S, Query(q): Q, headers: HeaderMap) -> Response {
    let pr = Params(&q);
    let list_of = |name: &str, default: &str| -> Vec<String> {
        q.get(name).map(String::as_str).unwrap_or(default).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };
    let topics = list_of("topics", "feed,chain");
    if let Some(bad) = topics.iter().find(|t| !aave_watch_protocol::keys::TOPICS.contains(&t.as_str())) {
        return fail(StatusCode::BAD_REQUEST, format!("{bad} is not a topic. Use feed, state, risk or chain."));
    }
    let chains: Result<Vec<u32>, _> = list_of("chain", "").iter().map(|c| c.parse::<u32>()).collect();
    let chains = p!(chains.map_err(|_| fail(StatusCode::BAD_REQUEST, "chain must be chain ids separated by commas.")));
    let opt = |v: String| if v.is_empty() { None } else { Some(v) };
    let filter = stream::Filter {
        chains, topics,
        spoke: opt(p!(pr.address("spoke"))), hub: opt(p!(pr.address("hub"))), user: opt(p!(pr.address("user"))),
        min_usd: if q.contains_key("min_usd") { Some(p!(pr.float("min_usd", 0.0))) } else { None },
    };
    let mut conn = match app.tip.conn().await {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::SERVICE_UNAVAILABLE, format!("Live updates are unavailable: {e}")),
    };
    if let Err(e) = app.tip.ping().await {
        return fail(StatusCode::SERVICE_UNAVAILABLE, format!("Live updates are unavailable: {e}"));
    }

    // Subscribe before replaying, so nothing published in between is lost.
    let mut live = app.relay.subscribe();
    let last = headers.get("last-event-id").and_then(|v| v.to_str().ok()).and_then(stream::parse_event_id)
        .map(|(s, i)| (s.to_string(), i.to_string()));
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(256);
    // One line at once, so the browser's connection opens without waiting for the first item.
    let _ = tx.try_send(Ok(Event::default().comment("connected")));
    tokio::spawn(async move {
        let mut seen: HashMap<String, (u64, u64)> = HashMap::new(); // newest replayed id per stream
        if let Some((last_stream, last_id)) = last {
            for item in app.relay.replay(&mut conn, &filter, &last_stream, &last_id).await.unwrap_or_default() {
                seen.insert(item.stream.clone(), stream::id_key(&item.id));
                if tx.send(Ok(sse_event(&item))).await.is_err() { return; }
            }
        }
        loop {
            match live.recv().await {
                Ok(item) => {
                    if !filter.wants(&item) { continue; }
                    if seen.get(&item.stream).is_some_and(|s| stream::id_key(&item.id) <= *s) { continue; }
                    if tx.send(Ok(sse_event(&item))).await.is_err() { return; } // client went away
                }
                // This client fell behind the buffer: tell it, so it can reload what it shows.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if tx.send(Ok(Event::default().event("lagged").data("{}"))).await.is_err() { return; }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });
    let mut res = Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))).into_response();
    res.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res.headers_mut().insert("x-accel-buffering", "no".parse().unwrap());
    res
}
