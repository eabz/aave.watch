//! The processor's tip: unsettled blocks from `raw:{chain_id}` become the latest-state keys, the
//! risk rankings and the `feed`, `state`, `risk` and `chain` streams in Dragonfly.
//! Spec: docs/data-pipeline.md, "Dragonfly". Same parsing as the settled path (`process_block`).

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use aave_watch_protocol::codec::Codec;
use aave_watch_protocol::{keys, registry};
use anyhow::Result;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use redis::streams::{StreamReadOptions, StreamReadReply};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::fetch::ClickHouse;
use crate::model::text;
use crate::process::{Rows, State, process_block};
use crate::tip::xadd;

pub async fn run_chain(chain: registry::Chain, ch: ClickHouse) -> Result<()> {
    let url = std::env::var("DRAGONFLY_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    // No response timeout: this connection waits in a blocking XREAD.
    let config = redis::aio::ConnectionManagerConfig::new().set_response_timeout(None);
    let mut db = ConnectionManager::new_with_config(redis::Client::open(url)?, config).await?;
    loop {
        if let Err(e) = session(&chain, &ch, &mut db).await {
            eprintln!("{}: tip: {e:#}; starting again in 10s", chain.name);
        }
        sleep(Duration::from_secs(10)).await;
    }
}

async fn session(chain: &registry::Chain, ch: &ClickHouse, db: &mut ConnectionManager) -> Result<()> {
    let (id, codec) = (chain.chain_id, Codec::new());
    let stream = keys::raw(id);
    // Nothing to do until the fetcher is live; then start from full settled state.
    while !db.exists::<_, bool>(&stream).await? {
        sleep(Duration::from_secs(15)).await;
    }
    let mut world = ch.world(chain, &codec, u64::MAX).await?;
    let mut state = State::load(ch, id).await?;
    let mut settled = 0;
    let mut out = Rows::default();
    // The latest tables go through the same sink, under the name of the table they mirror.
    for (latest, table) in [
        ("hub_asset_latest", "core.hub_asset_state"),
        ("hub_spoke_latest", "core.hub_spoke_state"),
        ("reserve_latest", "core.reserve_state"),
        ("prices_latest", "core.prices"),
        ("user_account_latest", "core.user_account_state"),
    ] {
        let sql = format!(
            "SELECT * EXCEPT (inserted_at, block_time), toUnixTimestamp(t.block_time) AS block_time \
             FROM core.{latest} AS t FINAL WHERE chain_id = {id} FORMAT JSONEachRow"
        );
        for line in ch.query(&sql, String::new()).await?.lines() {
            out.0.entry(table).or_default().push(serde_json::from_str(line)?);
        }
    }
    // Names the API joins into /v1/events rows; feed items carry the same two.
    let mut names: HashMap<String, String> = HashMap::new();
    let sql = format!(
        "SELECT address, symbol FROM meta.tokens FINAL WHERE chain_id = {id} UNION ALL \
         SELECT address, name FROM meta.spokes FINAL WHERE chain_id = {id} FORMAT TSV"
    );
    for line in ch.query(&sql, String::new()).await?.lines() {
        names.extend(line.split_once('\t').map(|(address, name)| (address.to_string(), name.to_string())));
    }
    let loaded: usize = out.0.values().map(Vec::len).sum();
    sink(db, id, "", &out, &names, false).await?;
    eprintln!("{}: tip loaded with {loaded} settled rows", chain.name);

    let mut at: String = db.get::<_, Option<String>>(keys::tip_cursor(id)).await?.unwrap_or_else(|| "0".into());
    let options = StreamReadOptions::default().block(5_000).count(20);
    loop {
        let reply: StreamReadReply = db.xread_options(&[&stream], &[&at], &options).await?;
        for entry in reply.keys.into_iter().flat_map(|k| k.ids) {
            at = entry.id.clone();
            let Some(batch) = entry.get::<String>(keys::STREAM_FIELD).and_then(|j| serde_json::from_str::<Value>(&j).ok()) else { continue };
            if batch["type"] == "reorg" {
                // Clients drop unsettled items from that block; the fetcher publishes the
                // replacing blocks next, which overwrite the latest-state keys.
                xadd(db, &keys::chain(id), &json!({"v": keys::ENVELOPE_VERSION, "chain_id": id, "block": batch["from_block"], "type": "reorg", "from_block": batch["from_block"]})).await?;
                continue;
            }
            if batch["type"] == "registry" {
                // Contracts were decoded behind the cursor (process.rs, `late`): start again from
                // settled state, which now has them.
                let _: () = db.set(keys::tip_cursor(id), &at).await?;
                return Ok(());
            }
            // A Hub the tip admitted in this batch: its AddAsset logs are in the batch.
            world.hubs.extend(batch["hubs"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
            let head = &batch["head"];
            let Some(number) = head["number"].as_u64() else { continue };
            let empty = Vec::new();
            let mut blocks: BTreeMap<u64, Vec<&Value>> = BTreeMap::new();
            for log in batch["logs"].as_array().unwrap_or(&empty) {
                blocks.entry(log["block_number"].as_u64().unwrap_or(number)).or_default().push(log);
            }
            // The batch's calls were read at the head: its last block.
            let calls: Vec<&Value> = batch["calls"].as_array().unwrap_or(&empty).iter().filter(|c| c["ok"] == 1).collect();
            blocks.entry(number).or_default();
            let mut out = Rows::default();
            for (block, logs) in &blocks {
                process_block(&codec, &mut world, &mut state, id, *block, logs, if *block == number { &calls } else { &[] }, &mut out);
            }
            sink(db, id, text(head, "hash"), &out, &names, true).await?;

            let mut envelope = json!({"v": keys::ENVELOPE_VERSION, "chain_id": id, "block": number, "block_hash": head["hash"], "settled": false, "type": "head", "data": {"time": head["time"]}});
            xadd(db, &keys::chain(id), &envelope).await?;
            let now_settled = batch["settled"].as_u64().unwrap_or(0);
            if now_settled > settled {
                settled = now_settled;
                envelope["type"] = json!("settled");
                envelope["block"] = json!(settled);
                envelope["settled"] = json!(true);
                xadd(db, &keys::chain(id), &envelope).await?;
            }
            let fields = [("number", number.to_string()), ("hash", text(head, "hash").to_string()), ("time", utc(head["time"].as_u64().unwrap_or(0))), ("settled", settled.to_string())];
            let _: () = db.hset_multiple(keys::head(id), &fields).await?;
            let _: () = db.set(keys::tip_cursor(id), &at).await?;
        }
    }
}

/// Writes parsed rows to Dragonfly: the latest-state keys always, the streams when `publish`.
async fn sink(db: &mut ConnectionManager, chain_id: u32, block_hash: &str, out: &Rows, names: &HashMap<String, String>, publish: bool) -> Result<()> {
    let envelope = |kind: &str, name: &Value, row: &Value| {
        // Times leave the API as text (docs/api.md), in the stream as in the endpoints.
        let mut data = row.clone();
        data["block_time"] = json!(utc(plain(&row["block_time"]).parse().unwrap_or(0)));
        json!({"v": keys::ENVELOPE_VERSION, "chain_id": chain_id, "block": row["block_number"], "block_hash": block_hash,
               "log_index": row["log_index"], "settled": false, "type": kind, "name": name, "data": data})
    };
    for (table, rows) in &out.0 {
        for row in rows {
            let (spoke, hub) = (text(row, "spoke"), text(row, "hub"));
            let id = |key: &str| row[key].as_u64().or_else(|| row[key].as_str()?.parse().ok()).unwrap_or(0) as u32;
            match *table {
                "core.events" if publish => {
                    let mut event = row.clone();
                    // `args` is stored as JSON text; clients get the object, as from /v1/events.
                    event["args"] = serde_json::from_str(text(row, "args")).unwrap_or(Value::Null);
                    event["symbol"] = json!(names.get(text(row, "token")).map_or("", String::as_str));
                    event["spoke_name"] = json!(names.get(spoke).map_or("", String::as_str));
                    let entry = envelope("event", &row["name"], &event);
                    xadd(db, &keys::feed(chain_id), &entry).await?;
                    if row["name"] == "LiquidationCall" {
                        xadd(db, &keys::risk(chain_id), &entry).await?;
                    }
                }
                "core.hub_asset_state" | "core.hub_spoke_state" | "core.reserve_state" => {
                    let (kind, key) = match *table {
                        "core.hub_asset_state" => ("hub_asset", keys::hub_asset(chain_id, hub, id("asset_id"))),
                        "core.hub_spoke_state" => ("hub_spoke", keys::hub_spoke(chain_id, hub, id("asset_id"), spoke)),
                        _ => ("reserve", keys::reserve(chain_id, spoke, id("reserve_id"))),
                    };
                    let _: () = db.set(key, row.to_string()).await?;
                    if publish {
                        xadd(db, &keys::state(chain_id), &envelope(kind, &Value::Null, row)).await?;
                    }
                }
                "core.prices" => {
                    let _: () = db.hset(keys::prices(chain_id, spoke), id("reserve_id"), format!("{}:{}", row["price"], plain(&row["block_number"]))).await?;
                    if publish {
                        xadd(db, &keys::state(chain_id), &envelope("price", &Value::Null, row)).await?;
                    }
                }
                "core.user_account_state" => {
                    let user = text(row, "user");
                    let (health, debt) = (row["health_factor"].as_f64().unwrap_or(f64::INFINITY), row["debt_usd"].as_f64().unwrap_or(0.0));
                    let key = keys::user(chain_id, spoke, user);
                    let before: Option<f64> = db.hget(&key, "hf").await?;
                    let fields = [
                        ("hf", health.to_string()), ("collateral_usd", plain(&row["collateral_usd"])), ("debt_usd", debt.to_string()),
                        ("risk_premium", plain(&row["risk_premium"])), ("block", plain(&row["block_number"])),
                        ("time", utc(plain(&row["block_time"]).parse().unwrap_or(0))),
                    ];
                    let _: () = db.hset_multiple(&key, &fields).await?;
                    // Accounts with no debt are in neither ranking.
                    if debt > 0.0 && health.is_finite() {
                        let _: () = db.zadd(keys::by_health(chain_id, spoke), user, health).await?;
                        let _: () = db.zadd(keys::by_debt(chain_id, spoke), user, debt).await?;
                    } else {
                        let _: () = db.zrem(keys::by_health(chain_id, spoke), user).await?;
                        let _: () = db.zrem(keys::by_debt(chain_id, spoke), user).await?;
                    }
                    let (was, is) = (band(before.unwrap_or(f64::INFINITY)), band(health));
                    if publish && was != is {
                        let mut crossing = row.clone();
                        crossing["from_band"] = json!(was);
                        crossing["to_band"] = json!(is);
                        xadd(db, &keys::risk(chain_id), &envelope("band", &Value::Null, &crossing)).await?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// A scalar as plain text: ClickHouse writes 64-bit integers to JSON as strings, serde as numbers.
fn plain(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

/// The health band a factor falls in, by its upper bound. Thresholds are the placeholders of
/// docs/dashboard-data.md, open decision 2.
fn band(health: f64) -> &'static str {
    match health {
        h if h < 1.0 => "liquidatable",
        h if h < 1.1 => "below_1_1",
        h if h < 1.5 => "below_1_5",
        _ => "healthy",
    }
}

/// Unix seconds as `YYYY-MM-DD hh:mm:ss` UTC, the time format of the API.
pub(crate) fn utc(t: u64) -> String {
    // Days to civil date, Howard Hinnant's algorithm.
    let (days, secs) = ((t / 86_400) as i64, t % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_bands() {
        assert_eq!(utc(0), "1970-01-01 00:00:00");
        assert_eq!(utc(1_795_012_264), "2026-11-18 14:31:04");
        assert_eq!(utc(1_709_251_199), "2024-02-29 23:59:59"); // leap day
        assert_eq!(band(0.99), "liquidatable");
        assert_eq!(band(1.0), "below_1_1");
        assert_eq!(band(1.4999), "below_1_5");
        assert_eq!(band(f64::INFINITY), "healthy");
    }
}
