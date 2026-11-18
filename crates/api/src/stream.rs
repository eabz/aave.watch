//! Dragonfly stream consumer. One task reads every tip stream of every chain and fans the entries
//! out to the connected SSE clients; each client filters for itself.

use aave_watch_protocol::keys;
use redis::streams::{StreamRangeReply, StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// One stream entry, parsed once.
pub struct Item {
    pub stream: String, // "feed:1"
    pub topic: String,  // "feed"
    pub chain_id: u32,
    pub id: String,     // Dragonfly entry id, "1795012264330-0"
    pub json: String,   // the envelope, as written by the indexer
    pub value: Value,
}

impl Item {
    fn new(stream: &str, id: &str, json: String) -> Option<Self> {
        let (topic, chain) = stream.split_once(':')?;
        let value = serde_json::from_str(&json).ok()?;
        Some(Self { stream: stream.into(), topic: topic.into(), chain_id: chain.parse().ok()?, id: id.into(), json, value })
    }
}

/// What one client asked for.
#[derive(Default)]
pub struct Filter {
    pub chains: Vec<u32>,     // empty = all
    pub topics: Vec<String>,
    pub spoke: Option<String>,
    pub hub: Option<String>,
    pub user: Option<String>,
    pub min_usd: Option<f64>,
}

impl Filter {
    pub fn wants(&self, item: &Item) -> bool {
        let data = &item.value["data"];
        let is = |want: &Option<String>, key: &str| match want {
            None => true,
            Some(w) => data[key].as_str().is_some_and(|v| v.eq_ignore_ascii_case(w)),
        };
        (self.chains.is_empty() || self.chains.contains(&item.chain_id))
            && self.topics.iter().any(|t| *t == item.topic)
            && is(&self.spoke, "spoke")
            && is(&self.hub, "hub")
            && is(&self.user, "user")
            && match (self.min_usd, item.topic.as_str()) {
                (Some(min), "feed") => data["amount_usd"].as_f64().is_some_and(|v| v >= min),
                _ => true,
            }
    }
}

/// Entry ids are `milliseconds-sequence`; they order as that pair of numbers.
pub fn id_key(id: &str) -> (u64, u64) {
    let (ms, seq) = id.split_once('-').unwrap_or((id, "0"));
    (ms.parse().unwrap_or(0), seq.parse().unwrap_or(0))
}

/// Splits an SSE id (`feed:1/1795012264330-0`) into stream name and entry id.
pub fn parse_event_id(s: &str) -> Option<(&str, &str)> {
    let (stream, id) = s.split_once('/')?;
    (stream.contains(':') && id.contains('-')).then_some((stream, id))
}

pub struct Relay {
    tx: broadcast::Sender<Arc<Item>>,
    chains: Vec<u32>,
}

impl Relay {
    pub fn start(client: redis::Client, chains: Vec<u32>) -> Self {
        let (tx, _) = broadcast::channel(4096);
        let names: Vec<String> = chains.iter().flat_map(|&c| keys::TOPICS.iter().map(move |t| keys::stream(t, c))).collect();
        tokio::spawn(read_forever(client, names, tx.clone()));
        Self { tx, chains }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Item>> {
        self.tx.subscribe()
    }

    /// Entries a reconnecting client missed. For the stream its last id came from, everything
    /// after that id; for its other streams, everything from the same millisecond on (so an item
    /// may repeat; clients drop repeats by chain, block and log index).
    pub async fn replay(&self, c: &mut redis::aio::ConnectionManager, filter: &Filter, last_stream: &str, last_id: &str) -> Result<Vec<Item>, String> {
        let ms = id_key(last_id).0;
        let chains = if filter.chains.is_empty() { &self.chains } else { &filter.chains };
        let mut out = Vec::new();
        for &chain in chains {
            for topic in &filter.topics {
                let name = keys::stream(topic, chain);
                let start = if name == last_stream { format!("({last_id}") } else { format!("{ms}-0") };
                let reply: StreamRangeReply = c.xrange_count(&name, start, "+", 5000).await.map_err(|e| e.to_string())?;
                out.extend(reply.ids.into_iter().filter_map(|e| {
                    let json: String = e.get(keys::STREAM_FIELD)?;
                    Item::new(&name, &e.id, json)
                }).filter(|i| filter.wants(i)));
            }
        }
        out.sort_by_key(|i| id_key(&i.id));
        Ok(out)
    }
}

/// Reads all streams with one blocking XREAD, forever. A lost connection is retried every second;
/// the last id seen per stream is kept, so nothing published while reconnecting is skipped (within
/// the streams' one-hour retention).
async fn read_forever(client: redis::Client, names: Vec<String>, tx: broadcast::Sender<Arc<Item>>) {
    let mut last: HashMap<String, String> = names.iter().map(|n| (n.clone(), "$".to_string())).collect();
    let opts = StreamReadOptions::default().block(5000).count(500);
    loop {
        // a connection of its own: a blocking read would hold up every other command on a shared one
        let cfg = redis::AsyncConnectionConfig::new().set_response_timeout(None);
        let mut c = match client.get_multiplexed_async_connection_with_config(&cfg).await {
            Ok(c) => c,
            Err(_) => { tokio::time::sleep(std::time::Duration::from_secs(1)).await; continue; }
        };
        // "$" means "from now"; pin it to a real id so a reconnect resumes instead of jumping ahead
        for (name, id) in last.iter_mut() {
            if id == "$" {
                let tail: StreamRangeReply = match c.xrevrange_count(name, "+", "-", 1).await { Ok(t) => t, Err(_) => continue };
                *id = tail.ids.first().map(|e| e.id.clone()).unwrap_or_else(|| "0-0".to_string());
            }
        }
        loop {
            let ids: Vec<&String> = names.iter().map(|n| &last[n]).collect();
            let reply: StreamReadReply = match c.xread_options(&names, &ids, &opts).await {
                Ok(r) => r,
                Err(_) => break,
            };
            for key in reply.keys {
                for e in key.ids {
                    last.insert(key.key.clone(), e.id.clone());
                    let Some(json) = e.get::<String>(keys::STREAM_FIELD) else { continue };
                    if let Some(item) = Item::new(&key.key, &e.id, json) {
                        let _ = tx.send(Arc::new(item)); // no receivers is fine
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(stream: &str, id: &str, json: &str) -> Item {
        Item::new(stream, id, json.to_string()).unwrap()
    }

    #[test]
    fn filter_keeps_only_what_the_client_asked_for() {
        let borrow = item("feed:1", "10-0", r#"{"v":1,"chain_id":1,"type":"event","name":"Borrow","data":{"spoke":"0xAbC","user":"0x1","amount_usd":500.0}}"#);
        let head = item("chain:8453", "11-0", r#"{"v":1,"chain_id":8453,"type":"head","data":{}}"#);
        let all = Filter { topics: vec!["feed".into(), "chain".into()], ..Default::default() };
        assert!(all.wants(&borrow) && all.wants(&head));

        let eth_only = Filter { chains: vec![1], topics: vec!["feed".into(), "chain".into()], ..Default::default() };
        assert!(eth_only.wants(&borrow) && !eth_only.wants(&head));

        let feed_only = Filter { topics: vec!["feed".into()], ..Default::default() };
        assert!(!feed_only.wants(&head));

        let spoke = Filter { topics: vec!["feed".into()], spoke: Some("0xabc".into()), ..Default::default() };
        assert!(spoke.wants(&borrow));
        let other = Filter { topics: vec!["feed".into()], spoke: Some("0xdef".into()), ..Default::default() };
        assert!(!other.wants(&borrow));

        let big = Filter { topics: vec!["feed".into(), "chain".into()], min_usd: Some(1000.0), ..Default::default() };
        assert!(!big.wants(&borrow));
        assert!(big.wants(&head), "min_usd applies to feed items only");
    }

    #[test]
    fn ids_order_numerically_and_event_ids_round_trip() {
        assert!(id_key("9-5") < id_key("10-0"));
        assert!(id_key("10-2") < id_key("10-10"));
        assert_eq!(parse_event_id("feed:1/1795012264330-0"), Some(("feed:1", "1795012264330-0")));
        assert_eq!(parse_event_id("garbage"), None);
        assert!(Item::new("feed:1", "1-0", "not json".into()).is_none());
    }
}
