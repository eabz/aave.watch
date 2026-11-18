//! ABI-driven decoding of logs and view calls, and encoding of view calls. Everything is looked up
//! in the deployed ABIs (`abi/*.json`), so a new event or view needs no code here.
//! Decoded values are JSON: integers are decimal strings, addresses lowercase 0x hex.

use std::collections::HashMap;

pub use alloy_dyn_abi::DynSolValue;
use alloy_dyn_abi::{EventExt, FunctionExt, JsonAbiExt};
use alloy_json_abi::{Event, Function, JsonAbi, Param};
pub use alloy_primitives::U256;
use alloy_primitives::{Address, B256, hex};
use serde_json::{Map, Value};

use crate::abi;

/// Multicall2, called through a state override (it is not deployed on every chain), and the three
/// ERC-20 metadata views.
const EXTRA: &str = r#"[
{"type":"function","name":"tryAggregate","stateMutability":"view","inputs":[{"name":"requireSuccess","type":"bool"},{"name":"calls","type":"tuple[]","components":[{"name":"target","type":"address"},{"name":"callData","type":"bytes"}]}],"outputs":[{"name":"returnData","type":"tuple[]","components":[{"name":"success","type":"bool"},{"name":"returnData","type":"bytes"}]}]},
{"type":"function","name":"getCurrentBlockTimestamp","stateMutability":"view","inputs":[],"outputs":[{"name":"timestamp","type":"uint256"}]},
{"type":"function","name":"symbol","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"string"}]},
{"type":"function","name":"name","stateMutability":"view","inputs":[],"outputs":[{"name":"","type":"string"}]}
]"#;

/// Where Multicall2 is placed by the state override of every `eth_call` we make.
pub const MULTICALL: &str = "0x5ba1e12693dc8f9c48aad8770482f4739beed696";
/// Its runtime bytecode, as deployed on Ethereum (`eth_getCode`, 2026-11-18).
pub const MULTICALL_CODE: &str = include_str!("../abi/multicall2.hex");

pub struct Codec {
    events: HashMap<B256, (&'static str, Event)>,
    by_selector: HashMap<[u8; 4], Function>,
    by_name: HashMap<String, Function>,
}

impl Default for Codec {
    fn default() -> Self {
        Self::new()
    }
}

impl Codec {
    pub fn new() -> Self {
        let mut codec = Self { events: HashMap::new(), by_selector: HashMap::new(), by_name: HashMap::new() };
        // First ABI wins: the events two kinds share (Initialized, AuthorityUpdated) decode the same.
        for (kind, json) in [
            ("hub", abi::HUB),
            ("spoke", abi::SPOKE),
            ("tokenization", abi::TOKENIZATION_SPOKE),
            ("treasury", abi::TREASURY_SPOKE),
            ("oracle", abi::AAVE_ORACLE),
            ("ir_strategy", abi::INTEREST_RATE_STRATEGY),
            ("extra", EXTRA),
        ] {
            let parsed: JsonAbi = serde_json::from_str(json).expect("abi json");
            for event in parsed.events() {
                codec.events.entry(event.selector()).or_insert((kind, event.clone()));
            }
            for function in parsed.functions() {
                codec.by_selector.entry(function.selector().0).or_insert_with(|| function.clone());
                codec.by_name.entry(function.name.clone()).or_insert_with(|| function.clone());
            }
        }
        codec
    }

    /// A log as (contract kind, event name, arguments by name). `None` for an event no ABI has.
    pub fn log(&self, topics: &[&str], data: &str) -> Option<(&'static str, &str, Value)> {
        let topics = topics.iter().filter(|t| !t.is_empty()).map(|t| t.parse()).collect::<Result<Vec<B256>, _>>().ok()?;
        let (kind, event) = self.events.get(topics.first()?)?;
        let decoded = event.decode_log_parts(topics, &hex::decode(data).ok()?).ok()?;
        let (mut indexed, mut body) = (decoded.indexed.iter(), decoded.body.iter());
        let mut args = Map::new();
        for input in &event.inputs {
            let value = if input.indexed { indexed.next()? } else { body.next()? };
            args.insert(input.name.clone(), json(value, &input.components));
        }
        Some((kind, &event.name, Value::Object(args)))
    }

    /// Calldata (0x hex) for a view, by name. Panics on a name or argument list no ABI has: that
    /// is a bug in the caller, not in data.
    pub fn call(&self, name: &str, args: &[DynSolValue]) -> String {
        let function = self.by_name.get(name).unwrap_or_else(|| panic!("no view named {name}"));
        let input = function.abi_encode_input(args).unwrap_or_else(|e| panic!("{name}: {e}"));
        format!("0x{}", hex::encode(input))
    }

    /// A stored call as (view name, inputs by name, output). `args` is the calldata after the
    /// selector. One output is returned as itself, several as an array.
    pub fn result(&self, selector: &str, args: &str, result: &str) -> Option<(&str, Value, Value)> {
        let selector: [u8; 4] = hex::decode(selector).ok()?.try_into().ok()?;
        let function = self.by_selector.get(&selector)?;
        let inputs = function.abi_decode_input(&hex::decode(args).ok()?).ok()?;
        let named = function.inputs.iter().zip(&inputs).map(|(p, v)| (p.name.clone(), json(v, &p.components)));
        let raw = hex::decode(result).ok()?;
        let output = match function.abi_decode_output(&raw) {
            Ok(values) => {
                let mut out: Vec<Value> =
                    function.outputs.iter().zip(&values).map(|(p, v)| json(v, &p.components)).collect();
                if out.len() == 1 { out.remove(0) } else { Value::Array(out) }
            }
            // Old tokens answer symbol() and name() with bytes32.
            Err(_) if raw.len() == 32 && matches!(function.name.as_str(), "symbol" | "name") => {
                Value::String(String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string())
            }
            Err(_) => return None,
        };
        Some((&function.name, Value::Object(named.collect()), output))
    }

    /// One `eth_call` body for Multicall2: every call at the same block, a failing one does not
    /// fail the rest. The first sub-call asks the multicall itself for the block's timestamp.
    pub fn multicall(&self, calls: &[(String, String)]) -> String {
        let item = |target: &str, data: &str| {
            DynSolValue::Tuple(vec![address(target), DynSolValue::Bytes(hex::decode(data).expect("calldata"))])
        };
        let mut items = vec![item(MULTICALL, &self.call("getCurrentBlockTimestamp", &[]))];
        items.extend(calls.iter().map(|(target, data)| item(target, data)));
        self.call("tryAggregate", &[DynSolValue::Bool(false), DynSolValue::Array(items)])
    }

    /// The reply to `multicall`: the block's timestamp, then (ok, result hex) per call, in order.
    pub fn multicall_result(&self, reply: &str) -> Option<(u64, Vec<(bool, String)>)> {
        let raw = hex::decode(reply).ok()?;
        let decoded = self.by_name["tryAggregate"].abi_decode_output(&raw).ok()?;
        let mut items = decoded.first()?.as_array()?.iter().map(|item| {
            let pair = item.as_tuple()?;
            Some((pair[0].as_bool()?, pair[1].as_bytes()?))
        });
        let (ok, time) = items.next()??;
        if !ok || time.len() != 32 {
            return None;
        }
        let time = U256::from_be_slice(time).try_into().ok()?;
        let results = items.map(|i| i.map(|(ok, bytes)| (ok, format!("0x{}", hex::encode(bytes))))).collect::<Option<_>>()?;
        Some((time, results))
    }
}

pub fn address(hex: &str) -> DynSolValue {
    DynSolValue::Address(hex.parse::<Address>().expect("address"))
}

pub fn uint(n: u64) -> DynSolValue {
    DynSolValue::Uint(U256::from(n), 256)
}

pub fn uints(ns: &[u32]) -> DynSolValue {
    DynSolValue::Array(ns.iter().map(|n| uint(*n as u64)).collect())
}

fn json(value: &DynSolValue, components: &[Param]) -> Value {
    match value {
        DynSolValue::Bool(b) => Value::Bool(*b),
        DynSolValue::Uint(n, _) => Value::String(n.to_string()),
        DynSolValue::Int(n, _) => Value::String(n.to_string()),
        DynSolValue::Address(a) => Value::String(format!("{a:#x}")),
        DynSolValue::FixedBytes(word, size) => Value::String(format!("0x{}", hex::encode(&word[..*size]))),
        DynSolValue::Bytes(bytes) => Value::String(format!("0x{}", hex::encode(bytes))),
        DynSolValue::String(s) => Value::String(s.clone()),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => {
            Value::Array(items.iter().map(|i| json(i, components)).collect())
        }
        DynSolValue::Tuple(fields) => Value::Object(
            fields.iter().enumerate().map(|(i, f)| match components.get(i) {
                Some(p) => (p.name.clone(), json(f, &p.components)),
                None => (i.to_string(), json(f, &[])),
            }).collect(),
        ),
        other => Value::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_borrow_log_decodes_by_name() {
        let codec = Codec::new();
        let word = |n: u64| format!("{n:064x}");
        let topics = [
            abi::topic0("spoke", "Borrow").unwrap().to_string(),
            format!("0x{}", word(3)),
            format!("0x{:0>64}", "1111111111111111111111111111111111111111"),
            format!("0x{:0>64}", "AbCd111111111111111111111111111111111111"),
        ];
        let topics: Vec<&str> = topics.iter().map(String::as_str).collect();
        let data = format!("0x{}{}", word(500), word(1_000_000));
        let (kind, name, args) = codec.log(&topics, &data).unwrap();
        assert_eq!((kind, name), ("spoke", "Borrow"));
        assert_eq!(args["reserveId"], "3");
        assert_eq!(args["user"], "0xabcd111111111111111111111111111111111111");
        assert_eq!(args["drawnAmount"], "1000000");
        // an event no ABI has, and a truncated one, are skipped rather than guessed
        assert!(codec.log(&["0x00000000000000000000000000000000000000000000000000000000000000aa"], "0x").is_none());
        assert!(codec.log(&topics, "0x00").is_none());
    }

    #[test]
    fn a_call_round_trips_and_structs_keep_their_field_names() {
        let codec = Codec::new();
        let data = codec.call("getSpokeOwed", &[uint(7), address("0x94e7A5dCbE816e498b89aB752661904E2F56c485")]);
        let word = |n: u64| format!("{n:064x}");
        let (name, inputs, output) =
            codec.result(&data[..10], &format!("0x{}", &data[10..]), &format!("0x{}{}", word(11), word(22))).unwrap();
        assert_eq!(name, "getSpokeOwed");
        assert_eq!(inputs["assetId"], "7");
        assert_eq!(inputs["spoke"], "0x94e7a5dcbe816e498b89ab752661904e2f56c485");
        assert_eq!(output, serde_json::json!(["11", "22"]));

        let data = codec.call("getSpokeConfig", &[uint(7), address(MULTICALL)]);
        let reply = format!("0x{}{}{}{}{}", word(100), word(50), word(1000), word(1), word(0));
        let (_, _, config) = codec.result(&data[..10], &format!("0x{}", &data[10..]), &reply).unwrap();
        assert_eq!(config["addCap"], "100");
        assert_eq!(config["active"], true);
        assert_eq!(config["halted"], false);
        // a reverted call has no result to decode
        assert!(codec.result(&data[..10], &format!("0x{}", &data[10..]), "0x").is_none());
    }

    #[test]
    fn multicall_encodes_and_its_reply_decodes() {
        let codec = Codec::new();
        let body = codec.multicall(&[(MULTICALL.to_string(), "0x0f28c97d".to_string())]);
        assert!(body.starts_with("0x") && body.len() > 10);
        // reply built by hand: [(true, uint 1800000000), (false, 0x)]
        let w = |n: u64| format!("{n:064x}");
        let reply = format!(
            "0x{}{}{}{}{}{}{}{}{}{}{}",
            w(0x20), w(2), w(0x40), w(0xc0),
            w(1), w(0x40), w(0x20), w(1_800_000_000),
            w(0), w(0x40), w(0)
        );
        let (time, results) = codec.multicall_result(&reply).unwrap();
        assert_eq!(time, 1_800_000_000);
        assert_eq!(results, [(false, "0x".to_string())]);
        assert!(codec.multicall_result("0x").is_none());
    }
}
