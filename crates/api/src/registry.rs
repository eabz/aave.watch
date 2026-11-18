//! `aave-watch-api load-registry`: copies registry/*.toml into meta.chains, meta.hubs and
//! meta.spokes, so names and kinds are queryable before any block is indexed. Safe to run again:
//! the tables keep the newest row per key. Assets, reserves and tokens come from the chain, not
//! from here.

use crate::ch::ClickHouse;
use serde_json::{Value, json};

const CHAINS: &str = include_str!("../../../registry/chains.toml");
const SPOKES: &str = include_str!("../../../registry/spokes.toml");

fn text(v: &toml::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn int(v: &toml::Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_integer()).unwrap_or(0)
}

/// The rows to write, per table. Addresses are lowercased, as everywhere in ClickHouse.
pub fn rows() -> Vec<(&'static str, Vec<Value>)> {
    let chains: toml::Table = toml::from_str(CHAINS).expect("registry/chains.toml");
    let spokes: toml::Table = toml::from_str(SPOKES).expect("registry/spokes.toml");
    let (mut c, mut h, mut s) = (Vec::new(), Vec::new(), Vec::new());
    for chain in chains["chain"].as_array().expect("[[chain]]") {
        let id = int(chain, "chain_id");
        c.push(json!({ "chain_id": id, "name": text(chain, "name"), "block_time_ms": int(chain, "block_time_ms"),
            "settle_tag": text(chain, "settle_tag"), "explorer_url": text(chain, "explorer_url") }));
        for hub in chain["hubs"].as_array().expect("hubs") {
            h.push(json!({ "chain_id": id, "address": text(hub, "address").to_lowercase(), "name": text(hub, "name"),
                "first_block": int(hub, "first_block") }));
        }
    }
    for spoke in spokes["spoke"].as_array().expect("[[spoke]]") {
        s.push(json!({ "chain_id": int(spoke, "chain_id"), "address": text(spoke, "address").to_lowercase(),
            "name": text(spoke, "name"), "kind": text(spoke, "kind"), "oracle": text(spoke, "oracle").to_lowercase(),
            "first_block": 0 }));
    }
    vec![("meta.chains", c), ("meta.hubs", h), ("meta.spokes", s)]
}

pub async fn load(ch: &ClickHouse) -> Result<(), String> {
    for (table, rows) in rows() {
        ch.insert(table, &rows).await?;
        println!("{table}: {} rows", rows.len());
    }
    ch.command("SYSTEM RELOAD DICTIONARIES").await
}

#[cfg(test)]
mod tests {
    #[test]
    fn registry_rows_are_complete_and_lowercase() {
        let rows = super::rows();
        let count = |t: &str| rows.iter().find(|(name, _)| *name == t).unwrap().1.len();
        assert_eq!((count("meta.chains"), count("meta.hubs"), count("meta.spokes")), (4, 7, 78));
        for (_, table) in &rows {
            for r in table {
                if let Some(a) = r.get("address").and_then(|a| a.as_str()) {
                    assert!(a.starts_with("0x") && a.len() == 42 && a == a.to_lowercase(), "{a}");
                }
            }
        }
        let kinds: std::collections::BTreeSet<&str> = rows[2].1.iter().map(|r| r["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds.into_iter().collect::<Vec<_>>(), ["spoke", "tokenization", "treasury"]);
    }
}
