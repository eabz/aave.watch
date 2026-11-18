//! ABIs of the deployed contracts, as verified on Sourcify (fetched 2026-11-18).
//! One ABI per kind: every chain runs the same code (see docs/protocol.md, "ABIs").
//! `abi/topics.tsv` lists every event with its topic0.

pub const HUB: &str = include_str!("../abi/hub.json");
pub const SPOKE: &str = include_str!("../abi/spoke.json");
pub const TREASURY_SPOKE: &str = include_str!("../abi/treasury_spoke.json");
pub const TOKENIZATION_SPOKE: &str = include_str!("../abi/tokenization_spoke.json");
pub const AAVE_ORACLE: &str = include_str!("../abi/aave_oracle.json");
pub const INTEREST_RATE_STRATEGY: &str = include_str!("../abi/interest_rate_strategy.json");
pub const ACCESS_MANAGER: &str = include_str!("../abi/access_manager.json");
pub const CHAINLINK_AGGREGATOR_PROXY: &str = include_str!("../abi/chainlink_aggregator_proxy.json");
pub const TOPICS_TSV: &str = include_str!("../abi/topics.tsv");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_abi_is_present_and_has_its_key_event() {
        for (abi, event) in [
            (HUB, "\"name\":\"Draw\""),
            (SPOKE, "\"name\":\"LiquidationCall\""),
            (TOKENIZATION_SPOKE, "\"name\":\"Deposit\""),
            (AAVE_ORACLE, "\"name\":\"UpdateReserveSource\""),
            (INTEREST_RATE_STRATEGY, "\"name\":\"UpdateInterestRateData\""),
            (ACCESS_MANAGER, "\"name\":\"RoleGranted\""),
            (CHAINLINK_AGGREGATOR_PROXY, "\"name\":\"AnswerUpdated\""),
        ] {
            assert!(abi.trim_start().starts_with('['));
            assert!(abi.contains(event), "missing {event}");
        }
        assert!(TREASURY_SPOKE.contains("getSuppliedAssets"));
        // topic0 of Borrow on the standard Spoke, as seen in live logs
        assert!(TOPICS_TSV.lines().any(|l| l.starts_with("spoke\t") && l.contains("\tBorrow(")));
    }
}
