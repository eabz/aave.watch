CREATE TABLE IF NOT EXISTS meta.chains (
  chain_id UInt32,
  name String,
  block_time_ms UInt32,
  settle_tag LowCardinality(String), -- block tag a block must reach before it is written here
  explorer_url String,
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY chain_id;

CREATE TABLE IF NOT EXISTS meta.hubs (
  chain_id UInt32,
  address String,
  name String,
  first_block UInt64,
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, address);

CREATE TABLE IF NOT EXISTS meta.spokes (
  chain_id UInt32,
  address String,
  name String,
  kind LowCardinality(String), -- spoke, treasury, tokenization, babylon
  oracle String,
  first_block UInt64,
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, address);

CREATE TABLE IF NOT EXISTS meta.assets (
  chain_id UInt32,
  hub String,
  asset_id UInt32,
  token String,
  decimals UInt8,
  first_block UInt64,
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, hub, asset_id);

CREATE TABLE IF NOT EXISTS meta.reserves (
  chain_id UInt32,
  spoke String,
  reserve_id UInt32,
  hub String,
  asset_id UInt32,
  token String,
  decimals UInt8,
  first_block UInt64,
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, spoke, reserve_id);

-- symbol, name, decimals come from the token contract. canonical and logo come from registry files.
CREATE TABLE IF NOT EXISTS meta.tokens (
  chain_id UInt32,
  address String,
  symbol String,
  name String,
  decimals UInt8,
  canonical String, -- one id for the same asset on every chain, e.g. usdc
  logo String,      -- path under the web app's public folder, empty if none
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, address);

CREATE TABLE IF NOT EXISTS meta.price_sources (
  chain_id UInt32,
  spoke String,
  reserve_id UInt32,
  since_block UInt64,
  source String,
  kind LowCardinality(String), -- chainlink, other
  decimals UInt8,
  description String,
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, spoke, reserve_id, since_block);

CREATE TABLE IF NOT EXISTS meta.labels (
  chain_id UInt32,
  address String,
  label String,
  category LowCardinality(String),
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, address);

-- stream: live, backfill:hubs, backfill:spokes, reparse
CREATE TABLE IF NOT EXISTS meta.cursor (
  chain_id UInt32,
  stream LowCardinality(String),
  block_number UInt64,
  block_hash String,
  updated_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, stream);

CREATE TABLE IF NOT EXISTS meta.parser_runs (
  chain_id UInt32,
  parser_version UInt16,
  from_block UInt64,
  to_block UInt64,
  started_at DateTime,
  finished_at DateTime DEFAULT now()
) ENGINE = MergeTree ORDER BY (chain_id, parser_version, from_block);
