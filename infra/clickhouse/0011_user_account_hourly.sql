-- Wallet history: the last reading of each hour per account, for the charts of collateral, debt
-- and health factor over time. Spec: docs/data-pipeline.md, "Wallet history".
-- Keyed by wallet first: the API reads one wallet at a time. Read with FINAL.
-- Insert-triggered like the *_latest tables of 0006, not refreshable like the other rollups: the
-- newest block of the hour wins, so a row written twice cannot double count, and the daily
-- backfill's old rows reach it without a second pass.
CREATE TABLE IF NOT EXISTS core.user_account_hourly (
  chain_id UInt32,
  spoke LowCardinality(String),
  user String,
  hour DateTime('UTC'),
  block_number UInt64,
  block_time DateTime('UTC'),
  health_factor Float64,  -- +inf for an account with no debt
  collateral_usd Float64,
  debt_usd Float64,
  risk_premium Float64
) ENGINE = ReplacingMergeTree(block_number)
PARTITION BY (chain_id, toYYYYMM(hour))
ORDER BY (chain_id, user, spoke, hour);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.user_account_hourly_mv
TO core.user_account_hourly AS
SELECT chain_id, spoke, user, toStartOfHour(block_time) AS hour, block_number, block_time,
       health_factor, collateral_usd, debt_usd, risk_premium
FROM core.user_account_state;
