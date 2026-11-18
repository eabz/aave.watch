//! What the events of one chain have told us (which assets, Spokes and reserves exist) and, from
//! that, which views to read when an event touches something. Shared by the fetcher, which plans
//! calls with it, and the processor, which names rows with it. No I/O.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use aave_watch_protocol::codec::{Codec, address, uint, uints};
use aave_watch_protocol::registry;
use serde_json::Value;

/// A string argument of a decoded event or call ("" when absent).
pub fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args[key].as_str().unwrap_or("")
}

/// A stored log (a `raw.logs` row) as (contract kind, event name, arguments).
pub fn decode<'a>(codec: &'a Codec, log: &Value) -> Option<(&'static str, &'a str, Value)> {
    let topics = ["topic0", "topic1", "topic2", "topic3"].map(|t| text(log, t));
    codec.log(&topics, text(log, "data"))
}

/// A small integer argument (ids, keys, basis points). `None` when absent or too large.
pub fn int(args: &Value, key: &str) -> Option<u32> {
    args[key].as_str()?.parse().ok()
}

#[derive(Default)]
pub struct World {
    pub hubs: BTreeSet<String>,
    /// (hub, asset id) -> (underlying token, decimals)
    pub assets: BTreeMap<(String, u32), (String, u8)>,
    /// (hub, asset id, spoke): every Spoke line a Hub has announced, of any Spoke kind.
    pub hub_spokes: BTreeSet<(String, u32, String)>,
    /// (spoke, reserve id) -> (hub, asset id)
    pub reserves: BTreeMap<(String, u32), (String, u32)>,
    /// standard Spoke -> its AaveOracle
    pub oracles: BTreeMap<String, String>,
    /// tokenization Spoke -> (hub, asset id)
    pub vaults: BTreeMap<String, (String, u32)>,
    /// (spoke, reserve id, dynamic config key) -> (collateral factor, max liquidation bonus, liquidation fee)
    pub dynamic: HashMap<(String, u32, u32), (u32, u32, u32)>,
    /// (spoke, reserve id) -> the price source its oracle reads
    pub sources: BTreeMap<(String, u32), String>,
    /// Contracts watched for their logs only: interest rate strategies and access managers.
    pub others: BTreeSet<String>,
}

/// Something a view can be read for.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Entity {
    HubAsset(String, u32),
    HubSpoke(String, u32, String),
    Reserve(String, u32),
    User(String, String),
    UserReserve(String, String, u32),
}

/// The events `World` learns from: enough to rebuild it from `raw.logs` without the rest.
pub const REGISTRY_EVENTS: [(&str, &str); 11] = [
    ("hub", "UpdateAssetConfig"),
    ("hub", "AuthorityUpdated"),
    ("spoke", "UpdateReservePriceSource"),
    ("hub", "AddAsset"),
    ("hub", "AddSpoke"),
    ("spoke", "AddReserve"),
    ("spoke", "SetSpokeImmutables"),
    ("spoke", "AddDynamicReserveConfig"),
    ("spoke", "UpdateDynamicReserveConfig"),
    ("tokenization_spoke", "SetTokenizationSpokeImmutables"),
    ("aave_oracle", "SetSpoke"),
];

impl World {
    pub fn new(chain: &registry::Chain) -> Self {
        Self { hubs: chain.hubs.iter().map(|h| h.address.to_lowercase()).collect(), ..Self::default() }
    }

    /// Records what an event says exists. Returns the contracts seen for the first time whose
    /// constants are worth one read: `(address, view)`.
    pub fn apply(&mut self, contract: &str, name: &str, args: &Value) -> Vec<(String, &'static str)> {
        let mut new = Vec::new();
        let hub = self.hubs.contains(contract);
        match name {
            "AddAsset" if hub => {
                let token = text(args, "underlying").to_string();
                if let (Some(id), Some(decimals)) = (int(args, "assetId"), int(args, "decimals")) {
                    if !self.assets.values().any(|(t, _)| *t == token) {
                        new.extend(["symbol", "name"].map(|view| (token.clone(), view)));
                    }
                    self.assets.insert((contract.to_string(), id), (token, decimals as u8));
                }
            }
            "AddSpoke" if hub => {
                if let Some(id) = int(args, "assetId") {
                    self.hub_spokes.insert((contract.to_string(), id, text(args, "spoke").to_string()));
                }
            }
            "AddReserve" => {
                if let (Some(reserve), Some(asset)) = (int(args, "reserveId"), int(args, "assetId")) {
                    self.reserves.insert((contract.to_string(), reserve), (text(args, "hub").to_string(), asset));
                }
            }
            "SetSpokeImmutables" => {
                let oracle = text(args, "oracle").to_string();
                new.push((oracle.clone(), "decimals"));
                self.oracles.insert(contract.to_string(), oracle);
            }
            "UpdateAssetConfig" if hub => drop(self.others.insert(text(&args["config"], "irStrategy").to_string())),
            // Hubs and Spokes of every kind name the same access manager.
            "AuthorityUpdated" => drop(self.others.insert(text(args, "authority").to_string())),
            "UpdateReservePriceSource" => {
                if let Some(reserve) = int(args, "reserveId") {
                    self.sources.insert((contract.to_string(), reserve), text(args, "priceSource").to_string());
                }
            }
            "SetTokenizationSpokeImmutables" => {
                if let Some(id) = int(args, "assetId") {
                    self.vaults.insert(contract.to_string(), (text(args, "hub").to_string(), id));
                }
            }
            "AddDynamicReserveConfig" | "UpdateDynamicReserveConfig" => {
                let config = &args["config"];
                if let (Some(reserve), Some(key), Some(factor), Some(bonus), Some(fee)) = (
                    int(args, "reserveId"),
                    int(args, "dynamicConfigKey"),
                    int(config, "collateralFactor"),
                    int(config, "maxLiquidationBonus"),
                    int(config, "liquidationFee"),
                ) {
                    self.dynamic.insert((contract.to_string(), reserve, key), (factor, bonus, fee));
                }
            }
            _ => {}
        }
        new
    }

    /// What an event changed, so those views are read at its block. Call after `apply`.
    pub fn touched(&self, contract: &str, name: &str, args: &Value) -> Vec<Entity> {
        let mut out = Vec::new();
        let at = contract.to_string();
        if self.hubs.contains(contract) {
            let Some(id) = int(args, "assetId") else { return out };
            out.push(Entity::HubAsset(at.clone(), id));
            for key in ["spoke", "callerSpoke", "coveredSpoke", "sender", "receiver", "feeReceiver"] {
                let line = (at.clone(), id, text(args, key).to_string());
                if self.hub_spokes.contains(&line) {
                    out.push(Entity::HubSpoke(line.0, line.1, line.2));
                }
            }
        } else if self.oracles.contains_key(contract) {
            let user = text(args, "user");
            if !user.is_empty() {
                out.push(Entity::User(at.clone(), user.to_string()));
            }
            for key in ["reserveId", "collateralReserveId", "debtReserveId"] {
                let Some(id) = int(args, key) else { continue };
                out.push(Entity::Reserve(at.clone(), id));
                if !user.is_empty() {
                    out.push(Entity::UserReserve(at.clone(), user.to_string(), id));
                }
            }
            // A liquidator paid in shares now holds a supply position on the collateral reserve.
            if name == "LiquidationCall" && args["receiveShares"] == true {
                let liquidator = text(args, "liquidator").to_string();
                out.push(Entity::User(at.clone(), liquidator.clone()));
                out.extend(int(args, "collateralReserveId").map(|id| Entity::UserReserve(at, liquidator, id)));
            }
        }
        out
    }

    /// Every Hub asset, Spoke line and reserve: what the hourly heartbeat reads.
    pub fn everything(&self) -> Vec<Entity> {
        let assets = self.assets.keys().map(|(h, a)| Entity::HubAsset(h.clone(), *a));
        let lines = self.hub_spokes.iter().map(|(h, a, s)| Entity::HubSpoke(h.clone(), *a, s.clone()));
        let reserves = self.reserves.keys().map(|(s, r)| Entity::Reserve(s.clone(), *r));
        assets.chain(lines).chain(reserves).collect()
    }

    /// One `getReservesPrices` per oracle, for every reserve of its Spoke: `(target, calldata)`.
    pub fn price_calls(&self, codec: &Codec) -> Vec<(String, String)> {
        self.oracles
            .iter()
            .filter_map(|(spoke, oracle)| {
                let ids = self.reserve_ids(spoke);
                (!ids.is_empty()).then(|| (oracle.clone(), codec.call("getReservesPrices", &[uints(&ids)])))
            })
            .collect()
    }

    /// `latestRoundData` on every price source, for the time it was last updated. Sources that are
    /// not Chainlink feeds revert; that is recorded and means "no update time".
    pub fn source_calls(&self, codec: &Codec) -> Vec<(String, String)> {
        let sources: BTreeSet<&String> = self.sources.values().collect();
        sources.into_iter().map(|source| (source.clone(), codec.call("latestRoundData", &[]))).collect()
    }

    /// Every contract whose logs are fetched: Hubs, Spokes of every kind, oracles, interest rate
    /// strategies and access managers.
    pub fn watch(&self) -> BTreeSet<String> {
        let spokes = self.hub_spokes.iter().map(|(_, _, spoke)| spoke);
        let zero = format!("0x{}", "0".repeat(40));
        self.hubs.iter().chain(spokes).chain(self.oracles.values()).chain(&self.others).filter(|a| a.len() == 42 && **a != zero).cloned().collect()
    }

    pub fn reserve_ids(&self, spoke: &str) -> Vec<u32> {
        self.reserves.range((spoke.to_string(), 0)..=(spoke.to_string(), u32::MAX)).map(|((_, id), _)| *id).collect()
    }
}

impl Entity {
    /// The struct getters that describe this entity: `(target, calldata)`. No field getters.
    pub fn calls(&self, codec: &Codec) -> Vec<(String, String)> {
        let (target, views, args): (&String, &[&str], _) = match self {
            Entity::HubAsset(hub, id) => {
                (hub, &["getAsset", "getAddedAssets", "getAssetOwed", "getAssetAccruedFees"], vec![uint(*id as u64)])
            }
            Entity::HubSpoke(hub, id, spoke) => {
                (hub, &["getSpoke", "getSpokeAddedAssets", "getSpokeOwed"], vec![uint(*id as u64), address(spoke)])
            }
            Entity::Reserve(spoke, id) => (
                spoke,
                &["getReserve", "getReserveConfig", "getReserveSuppliedAssets", "getReserveSuppliedShares", "getReserveDebt"],
                vec![uint(*id as u64)],
            ),
            Entity::User(spoke, user) => (spoke, &["getUserAccountData", "getUserLastRiskPremium"], vec![address(user)]),
            Entity::UserReserve(spoke, user, id) => (
                spoke,
                &["getUserPosition", "getUserSuppliedAssets", "getUserDebt", "getUserReserveStatus"],
                vec![uint(*id as u64), address(user)],
            ),
        };
        views.iter().map(|view| (target.clone(), codec.call(view, &args))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HUB: &str = "0xcca852bc40e560adc3b1cc58ca5b55638ce826c9";
    const SPOKE: &str = "0x94e7a5dcbe816e498b89ab752661904e2f56c485";
    const ORACLE: &str = "0x6b3800e96bb4aa38a879b2c143b6e2f5f2687b77";
    const USER: &str = "0x1111111111111111111111111111111111111111";

    fn world() -> World {
        let mut w = World { hubs: [HUB.to_string()].into(), ..World::default() };
        let token = json!({"assetId": "3", "underlying": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "decimals": "18"});
        // a token is read once, however many Hubs list it
        assert_eq!(w.apply(HUB, "AddAsset", &token).len(), 2);
        assert!(w.apply(HUB, "AddAsset", &token).is_empty());
        w.apply(HUB, "AddSpoke", &json!({"assetId": "3", "spoke": SPOKE}));
        assert_eq!(w.apply(SPOKE, "SetSpokeImmutables", &json!({"oracle": ORACLE})), [(ORACLE.to_string(), "decimals")]);
        w.apply(SPOKE, "AddReserve", &json!({"reserveId": "0", "assetId": "3", "hub": HUB}));
        w.apply(SPOKE, "AddReserve", &json!({"reserveId": "5", "assetId": "3", "hub": HUB}));
        w
    }

    #[test]
    fn events_name_what_to_read() {
        let w = world();
        let draw = w.touched(HUB, "Draw", &json!({"assetId": "3", "spoke": SPOKE}));
        assert_eq!(draw, [Entity::HubAsset(HUB.into(), 3), Entity::HubSpoke(HUB.into(), 3, SPOKE.into())]);
        // an address that is not a Spoke line of that asset is not read as one
        assert_eq!(w.touched(HUB, "Draw", &json!({"assetId": "3", "spoke": USER})).len(), 1);

        let borrow = w.touched(SPOKE, "Borrow", &json!({"reserveId": "5", "user": USER, "caller": USER}));
        assert_eq!(
            borrow,
            [Entity::User(SPOKE.into(), USER.into()), Entity::Reserve(SPOKE.into(), 5), Entity::UserReserve(SPOKE.into(), USER.into(), 5)]
        );
        let liquidation = json!({"collateralReserveId": "0", "debtReserveId": "5", "user": USER, "liquidator": HUB, "receiveShares": true});
        assert_eq!(w.touched(SPOKE, "LiquidationCall", &liquidation).len(), 7);
        // a contract we know nothing about touches nothing
        assert!(w.touched(USER, "Borrow", &json!({"reserveId": "5", "user": USER})).is_empty());
    }

    #[test]
    fn the_heartbeat_reads_everything_and_prices_come_one_call_per_oracle() {
        let w = world();
        let codec = Codec::new();
        assert_eq!(w.everything().len(), 4); // one asset, one Spoke line, two reserves
        assert_eq!(w.reserve_ids(SPOKE), [0, 5]);
        // watched: the Hub, its Spoke, the Spoke's oracle; then a strategy and an access manager
        assert_eq!(w.watch().len(), 3);
        let mut w = w;
        w.apply(HUB, "UpdateAssetConfig", &json!({"assetId": "3", "config": {"irStrategy": USER}}));
        w.apply(SPOKE, "AuthorityUpdated", &json!({"authority": "0x0000000000000000000000000000000000000000"}));
        assert_eq!(w.watch().len(), 4); // the zero address is never watched
        w.apply(SPOKE, "UpdateReservePriceSource", &json!({"reserveId": "0", "priceSource": USER}));
        w.apply(SPOKE, "UpdateReservePriceSource", &json!({"reserveId": "5", "priceSource": USER}));
        assert_eq!(w.source_calls(&codec).len(), 1); // one read per source, not per reserve
        let prices = w.price_calls(&codec);
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].0, ORACLE);
        let calls = Entity::UserReserve(SPOKE.into(), USER.into(), 5).calls(&codec);
        assert_eq!(calls.len(), 4);
        assert!(calls.iter().all(|(target, data)| target == SPOKE && data.len() == 2 + 8 + 128));
    }
}
