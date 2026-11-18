//! Dragonfly key and stream names. The one place they are spelled; see docs/data-pipeline.md.
//! Every name starts with its kind, then the chain id. Addresses are lowercase 0x hex.

/// Streams keep one hour of entries (`XADD ... MINID ~ <now - RETENTION_MS>`), longer than any
/// chain takes to settle a block, so a reconnecting client can always resume from its last id.
pub const STREAM_RETENTION_MS: u64 = 60 * 60 * 1000;

/// TTL of the single-indexer lock, renewed while the indexer runs.
pub const LOCK_TTL_MS: u64 = 15_000;
pub const LOCK_INDEXER: &str = "lock:indexer";

/// Version of the JSON envelope written to every stream entry, in its `v` field.
pub const ENVELOPE_VERSION: u8 = 1;
/// Every stream entry has exactly one field with this name; its value is the JSON envelope.
pub const STREAM_FIELD: &str = "json";
/// The tip topics the API relays, in the order it names them.
pub const TOPICS: [&str; 4] = ["feed", "state", "risk", "chain"];
/// Stream name of a topic on a chain, e.g. `stream("feed", 1) == "feed:1"`.
pub fn stream(topic: &str, chain_id: u32) -> String { format!("{topic}:{chain_id}") }

// Streams, one set per chain.
pub fn feed(chain_id: u32) -> String { format!("feed:{chain_id}") }
pub fn state(chain_id: u32) -> String { format!("state:{chain_id}") }
pub fn risk(chain_id: u32) -> String { format!("risk:{chain_id}") }
pub fn chain(chain_id: u32) -> String { format!("chain:{chain_id}") }

/// One batch of unsettled blocks as fetched. Written by the fetcher, read by the processor.
pub fn raw(chain_id: u32) -> String { format!("raw:{chain_id}") }
/// String: the last `raw:{chain_id}` entry the processor has turned into tip state.
pub fn tip_cursor(chain_id: u32) -> String { format!("tipcursor:{chain_id}") }

// Latest state.
/// Hash: number, hash, time, settled.
pub fn head(chain_id: u32) -> String { format!("head:{chain_id}") }
/// String (JSON): latest hub asset row.
pub fn hub_asset(chain_id: u32, hub: &str, asset_id: u32) -> String {
    format!("hub:{chain_id}:{hub}:{asset_id}")
}
/// String (JSON): a Spoke's line on a Hub asset (caps, usage, flags).
pub fn hub_spoke(chain_id: u32, hub: &str, asset_id: u32, spoke: &str) -> String {
    format!("hubspoke:{chain_id}:{hub}:{asset_id}:{spoke}")
}
/// String (JSON): latest reserve row.
pub fn reserve(chain_id: u32, spoke: &str, reserve_id: u32) -> String {
    format!("reserve:{chain_id}:{spoke}:{reserve_id}")
}
/// Hash: reserve_id -> "price:block".
pub fn prices(chain_id: u32, spoke: &str) -> String { format!("price:{chain_id}:{spoke}") }
/// Hash: hf, collateral_usd, debt_usd, risk_premium, block, time (`YYYY-MM-DD hh:mm:ss`, UTC).
pub fn user(chain_id: u32, spoke: &str, user: &str) -> String {
    format!("user:{chain_id}:{spoke}:{user}")
}

// Sorted sets for the live risk view. Accounts with no debt are not members.
/// Score: health factor. Member: user.
pub fn by_health(chain_id: u32, spoke: &str) -> String { format!("hf:{chain_id}:{spoke}") }
/// Score: debt in USD. Member: user.
pub fn by_debt(chain_id: u32, spoke: &str) -> String { format!("debt:{chain_id}:{spoke}") }
/// Score: next block to read at. Member: `due_member`. The scheduler's queue.
pub fn due(chain_id: u32) -> String { format!("due:{chain_id}") }
pub fn due_member(spoke: &str, user: &str) -> String { format!("{spoke}:{user}") }

/// String with a TTL: cached ClickHouse result for a heavy endpoint.
pub fn query_cache(hash: &str) -> String { format!("q:{hash}") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable_and_chain_scoped() {
        assert_eq!(feed(1), "feed:1");
        assert_eq!(chain(8453), "chain:8453");
        assert_eq!(hub_asset(1, "0xcca8", 3), "hub:1:0xcca8:3");
        assert_eq!(hub_spoke(1, "0xcca8", 3, "0x94e7"), "hubspoke:1:0xcca8:3:0x94e7");
        assert_eq!(user(5042, "0x18dd", "0xabc"), "user:5042:0x18dd:0xabc");
        assert_eq!(by_health(43114, "0x01d7"), "hf:43114:0x01d7");
        // no two kinds of key for the same chain may collide
        let all = [feed(1), state(1), risk(1), chain(1), head(1), due(1)];
        let mut sorted = all.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len());
    }
}
