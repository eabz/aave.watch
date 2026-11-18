-- One row per decoded log, every event type in one table.
CREATE TABLE IF NOT EXISTS core.events (
  chain_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  log_index UInt32,
  tx_hash String,
  contract LowCardinality(String),
  kind LowCardinality(String), -- hub, spoke, oracle, ir_strategy
  name LowCardinality(String), -- Borrow, LiquidationCall, UpdateSpokeConfig, ...
  hub LowCardinality(String),
  spoke LowCardinality(String),
  asset_id Nullable(UInt32),
  reserve_id Nullable(UInt32),
  user String,
  caller String,
  amount UInt256,
  shares UInt256,
  token LowCardinality(String),
  price Float64,      -- oracle price at this block, USD
  amount_usd Float64,
  args String CODEC(ZSTD(3)), -- every decoded argument, JSON
  parser_version UInt16,
  inserted_at DateTime DEFAULT now(),
  INDEX ix_tx tx_hash TYPE bloom_filter GRANULARITY 4,
  PROJECTION by_user (SELECT * ORDER BY chain_id, user, block_number, log_index),
  PROJECTION by_place (SELECT * ORDER BY chain_id, spoke, hub, name, block_number, log_index)
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, block_number, log_index)
SETTINGS deduplicate_merge_projection_mode = 'rebuild';
