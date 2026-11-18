-- "The current row for every X". Same columns as the history table, one row per entity, newest block
-- wins. Fed by a materialized view; replacing is idempotent, so a block written twice is harmless.
-- Read with FINAL.
CREATE TABLE IF NOT EXISTS core.hub_asset_latest AS core.hub_asset_state
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, hub, asset_id);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.hub_asset_latest_mv TO core.hub_asset_latest
AS SELECT * FROM core.hub_asset_state;

CREATE TABLE IF NOT EXISTS core.hub_spoke_latest AS core.hub_spoke_state
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, hub, asset_id, spoke);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.hub_spoke_latest_mv TO core.hub_spoke_latest
AS SELECT * FROM core.hub_spoke_state;

CREATE TABLE IF NOT EXISTS core.reserve_latest AS core.reserve_state
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, spoke, reserve_id);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.reserve_latest_mv TO core.reserve_latest
AS SELECT * FROM core.reserve_state;

CREATE TABLE IF NOT EXISTS core.prices_latest AS core.prices
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, spoke, reserve_id);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.prices_latest_mv TO core.prices_latest
AS SELECT * FROM core.prices;

CREATE TABLE IF NOT EXISTS core.asset_prices_latest AS core.asset_prices
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, hub, asset_id);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.asset_prices_latest_mv TO core.asset_prices_latest
AS SELECT * FROM core.asset_prices;

CREATE TABLE IF NOT EXISTS core.user_reserve_latest AS core.user_reserve_state
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, spoke, user, reserve_id);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.user_reserve_latest_mv TO core.user_reserve_latest
AS SELECT * FROM core.user_reserve_state;

-- Ordered by spoke then user; the health factor scans (histogram, at risk) read a whole Spoke.
CREATE TABLE IF NOT EXISTS core.user_account_latest AS core.user_account_state
ENGINE = ReplacingMergeTree(block_number) ORDER BY (chain_id, spoke, user);
CREATE MATERIALIZED VIEW IF NOT EXISTS core.user_account_latest_mv TO core.user_account_latest
AS SELECT * FROM core.user_account_state;
