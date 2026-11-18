-- One row is one read of contract views at one block. Amounts are raw units; *_usd come from enrich.
CREATE TABLE IF NOT EXISTS core.hub_asset_state (
  chain_id UInt32,
  hub LowCardinality(String),
  asset_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  liquidity UInt256,
  swept UInt256,
  added_shares UInt256,
  added_assets UInt256,
  drawn_shares UInt256,
  premium_shares UInt256,
  premium_offset_ray Int256,
  drawn_owed UInt256,
  premium_owed UInt256,
  deficit_ray UInt256,
  realized_fees UInt256,
  accrued_fees UInt256,
  drawn_index UInt256,
  drawn_rate UInt256,
  liquidity_fee UInt16,
  ir_strategy LowCardinality(String),
  reinvestment_controller LowCardinality(String),
  fee_receiver LowCardinality(String),
  usage Float64,          -- drawn / (liquidity + drawn + swept)
  drawn_rate_apr Float64,
  price Float64,
  added_usd Float64,
  owed_usd Float64,
  liquidity_usd Float64,
  reason LowCardinality(String),
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, hub, asset_id, block_number);

CREATE TABLE IF NOT EXISTS core.hub_spoke_state (
  chain_id UInt32,
  hub LowCardinality(String),
  asset_id UInt32,
  spoke LowCardinality(String),
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  added_shares UInt256,
  added_assets UInt256,
  drawn_shares UInt256,
  premium_shares UInt256,
  premium_offset_ray Int256,
  drawn_owed UInt256,
  premium_owed UInt256,
  deficit_ray UInt256,
  add_cap UInt64,
  draw_cap UInt64,
  risk_premium_threshold UInt32,
  active UInt8,
  halted UInt8,
  price Float64,
  added_usd Float64,
  owed_usd Float64,
  reason LowCardinality(String),
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, hub, asset_id, spoke, block_number);

CREATE TABLE IF NOT EXISTS core.reserve_state (
  chain_id UInt32,
  spoke LowCardinality(String),
  reserve_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  hub LowCardinality(String),
  asset_id UInt32,
  supplied_shares UInt256,
  supplied_assets UInt256,
  drawn_debt UInt256,
  premium_debt UInt256,
  collateral_risk UInt32,
  paused UInt8,
  frozen UInt8,
  borrowable UInt8,
  receive_shares_enabled UInt8,
  dynamic_config_key UInt32,
  collateral_factor UInt16,
  max_liquidation_bonus UInt32,
  liquidation_fee UInt16,
  price Float64,
  supplied_usd Float64,
  debt_usd Float64,
  reason LowCardinality(String),
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, spoke, reserve_id, block_number);

-- A row only when the price changed. price_raw is in the oracle's own decimals.
CREATE TABLE IF NOT EXISTS core.prices (
  chain_id UInt32,
  spoke LowCardinality(String),
  reserve_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  price_raw UInt256,
  price Float64,
  source LowCardinality(String),
  source_updated_at Nullable(DateTime('UTC')), -- from latestRoundData where the feed has it
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, spoke, reserve_id, block_number);

-- Derived: one price per Hub asset (median of its Spokes' prices on that chain).
CREATE TABLE IF NOT EXISTS core.asset_prices (
  chain_id UInt32,
  hub LowCardinality(String),
  asset_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  price Float64,
  sources UInt8,
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, hub, asset_id, block_number);

CREATE TABLE IF NOT EXISTS core.user_reserve_state (
  chain_id UInt32,
  spoke LowCardinality(String),
  user String,
  reserve_id UInt32,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  supplied_shares UInt256,
  supplied_assets UInt256,
  drawn_shares UInt256,
  premium_shares UInt256,
  premium_offset_ray Int256,
  drawn_debt UInt256,
  premium_debt UInt256,
  is_collateral UInt8,
  is_borrowing UInt8,
  dynamic_config_key UInt32,
  price Float64,
  supplied_usd Float64,
  debt_usd Float64,
  reason LowCardinality(String),
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, spoke, user, reserve_id, block_number);

-- getUserAccountData. *_raw keep the contract's scale; the Float64 columns are for queries.
-- health_factor is +inf for an account with no debt.
CREATE TABLE IF NOT EXISTS core.user_account_state (
  chain_id UInt32,
  spoke LowCardinality(String),
  user String,
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  health_factor_raw UInt256,
  risk_premium_raw UInt256,
  last_risk_premium_raw UInt256, -- the premium the user is actually charged (getUserLastRiskPremium)
  avg_collateral_factor_raw UInt256,
  total_collateral_value_raw UInt256,
  total_debt_value_ray UInt256,
  active_collateral_count UInt16,
  borrow_count UInt16,
  health_factor Float64,
  risk_premium Float64,
  last_risk_premium Float64,
  collateral_usd Float64,
  debt_usd Float64,
  reason LowCardinality(String),
  parser_version UInt16,
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, spoke, user, block_number);
