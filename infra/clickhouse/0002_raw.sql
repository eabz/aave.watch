-- Addresses and hashes are lowercase 0x hex everywhere.
CREATE TABLE IF NOT EXISTS raw.blocks (
  chain_id UInt32,
  number UInt64 CODEC(Delta, ZSTD),
  hash String,
  parent_hash String,
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, number);

-- Ordered by contract and event: reparse reads "every Borrow log of this Spoke", not "block N".
CREATE TABLE IF NOT EXISTS raw.logs (
  chain_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  block_hash String,
  tx_hash String,
  tx_index UInt32,
  log_index UInt32,
  address LowCardinality(String),
  topic0 LowCardinality(String),
  topic1 String,
  topic2 String,
  topic3 String,
  data String CODEC(ZSTD(3)),
  inserted_at DateTime DEFAULT now(),
  INDEX ix_tx tx_hash TYPE bloom_filter GRANULARITY 4
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, address, topic0, block_number, log_index);

-- One eth_call result, as returned. args is the calldata after the 4-byte selector.
CREATE TABLE IF NOT EXISTS raw.calls (
  chain_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  target LowCardinality(String),
  selector LowCardinality(String),
  args String,
  ok UInt8,
  result String CODEC(ZSTD(3)),
  reason LowCardinality(String), -- event, heartbeat, tier, backfill
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, target, selector, args, block_number);
