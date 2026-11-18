//! The chain and Spoke registry, compiled in from `registry/*.toml`.
//! Only the fields code needs are read; the files carry more for people and for `meta.*`.

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct Chain {
    pub chain_id: u32,
    pub name: String,
    /// Block the earliest watched contract was deployed in: where the fetcher starts.
    pub start_block: u64,
    pub block_time_ms: u64,
    /// Block tag a block must reach before it is written to ClickHouse.
    pub settle_tag: String,
    pub hubs: Vec<Hub>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Hub {
    pub name: String,
    pub address: String,
    pub first_block: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Spoke {
    pub chain_id: u32,
    pub address: String,
    /// Standard Spokes only.
    pub oracle: Option<String>,
}

/// An Aave v3 market (registry/v3.toml), read for totals only.
#[derive(Deserialize, Clone, Debug)]
pub struct V3Market {
    pub chain_id: u32,
    pub chain: String,
    pub market: String,
    /// PoolAddressesProvider: names the market's oracle and data provider at any block.
    pub provider: String,
    pub pool: String,
}

#[derive(Deserialize)]
struct V3 {
    market: Vec<V3Market>,
}

pub fn v3_markets() -> Vec<V3Market> {
    toml::from_str::<V3>(include_str!("../../../registry/v3.toml")).expect("registry/v3.toml").market
}

#[derive(Deserialize)]
struct Chains {
    chain: Vec<Chain>,
}

#[derive(Deserialize)]
struct Spokes {
    spoke: Vec<Spoke>,
}

pub fn chains() -> Vec<Chain> {
    toml::from_str::<Chains>(include_str!("../../../registry/chains.toml"))
        .expect("registry/chains.toml")
        .chain
}

pub fn spokes() -> Vec<Spoke> {
    toml::from_str::<Spokes>(include_str!("../../../registry/spokes.toml"))
        .expect("registry/spokes.toml")
        .spoke
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_files_parse_and_agree() {
        let chains = chains();
        assert!(chains.iter().any(|c| c.chain_id == 1 && c.settle_tag == "finalized"));
        assert!(chains.iter().all(|c| !c.hubs.is_empty()));
        // a start after a Hub's first block would skip that Hub's first logs
        assert!(chains.iter().all(|c| c.hubs.iter().all(|h| c.start_block <= h.first_block)));
        let spokes = spokes();
        assert!(spokes.iter().any(|s| s.oracle.is_some()));
        // every Spoke belongs to a chain we know
        assert!(spokes.iter().all(|s| chains.iter().any(|c| c.chain_id == s.chain_id)));
        let v3 = v3_markets();
        assert_eq!(v3.iter().filter(|m| m.chain_id == 1).count(), 4);
        assert!(v3.iter().all(|m| m.provider.len() == 42 && m.pool.len() == 42 && !m.chain.is_empty()));
    }
}
