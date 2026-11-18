-- Derived data: numbers computed from core.* that several panels need. Views read at query time;
-- rollups follow the pattern of 0007 (refreshable, appended, ReplacingMergeTree on the bucket).

-- One row per LiquidationCall with both sides valued. Debt is valued by the event's own price (its
-- debt reserve); collateral by the Spoke's price for the collateral reserve at that block.
-- bonus_usd is collateral seized minus debt repaid (docs/dashboard-data.md, page 6); fee_usd is
-- the part of the seized collateral that did not go to the liquidator.
CREATE VIEW IF NOT EXISTS core.liquidations AS
SELECT e.chain_id AS chain_id, e.block_number AS block_number, e.block_time AS block_time, e.log_index AS log_index,
       e.tx_hash AS tx_hash, e.spoke AS spoke, e.hub AS hub, e.user AS user, e.liquidator AS liquidator,
       e.debt_reserve_id AS debt_reserve_id, e.collateral_reserve_id AS collateral_reserve_id,
       e.debt_token AS debt_token, r.token AS collateral_token, e.receive_shares AS receive_shares,
       e.debt_amount AS debt_amount, e.collateral_amount AS collateral_amount,
       e.debt_usd AS debt_usd,
       toFloat64(e.collateral_amount) / pow(10, r.decimals) * p.price AS collateral_usd,
       collateral_usd - e.debt_usd AS bonus_usd,
       if(e.shares_liquidated = 0, 0, collateral_usd * (1 - toFloat64(e.shares_to_liquidator) / toFloat64(e.shares_liquidated))) AS fee_usd
FROM (
  SELECT chain_id, block_number, block_time, log_index, tx_hash, toString(spoke) AS spoke, toString(hub) AS hub, user, caller AS liquidator,
         assumeNotNull(reserve_id) AS debt_reserve_id,
         toUInt32OrZero(JSONExtractString(args, 'collateralReserveId')) AS collateral_reserve_id,
         toString(token) AS debt_token, JSONExtractBool(args, 'receiveShares') AS receive_shares,
         amount AS debt_amount, amount_usd AS debt_usd,
         toUInt256OrZero(JSONExtractString(args, 'collateralAmountRemoved')) AS collateral_amount,
         toUInt256OrZero(JSONExtractString(args, 'collateralSharesLiquidated')) AS shares_liquidated,
         toUInt256OrZero(JSONExtractString(args, 'collateralSharesToLiquidator')) AS shares_to_liquidator
  FROM core.events FINAL WHERE name = 'LiquidationCall'
) AS e
LEFT JOIN (SELECT chain_id, spoke, reserve_id, token, decimals FROM meta.reserves FINAL) AS r
  ON r.chain_id = e.chain_id AND r.spoke = e.spoke AND r.reserve_id = e.collateral_reserve_id
ASOF LEFT JOIN (SELECT chain_id, toString(spoke) AS spoke, reserve_id, block_number, price FROM core.prices) AS p
  ON p.chain_id = e.chain_id AND p.spoke = e.spoke AND p.reserve_id = e.collateral_reserve_id AND p.block_number <= e.block_number;

CREATE TABLE IF NOT EXISTS core.liquidations_daily (
  chain_id UInt32,
  day Date,
  spoke LowCardinality(String),
  collateral_token LowCardinality(String),
  debt_token LowCardinality(String),
  n UInt64,
  users UInt64,
  liquidators UInt64,
  debt_usd Float64,
  collateral_usd Float64,
  bonus_usd Float64,
  fee_usd Float64,
  refreshed_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(refreshed_at)
PARTITION BY (chain_id, toYYYYMM(day))
ORDER BY (chain_id, day, spoke, collateral_token, debt_token);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.liquidations_daily_mv
REFRESH EVERY 5 MINUTE APPEND TO core.liquidations_daily AS
SELECT chain_id, toDate(block_time) AS day, spoke, collateral_token, debt_token,
       count() AS n, uniqExact(user) AS users, uniqExact(liquidator) AS liquidators,
       sum(l.debt_usd) AS debt_usd, sum(l.collateral_usd) AS collateral_usd, sum(l.bonus_usd) AS bonus_usd, sum(l.fee_usd) AS fee_usd,
       now() AS refreshed_at
FROM core.liquidations AS l
WHERE block_time >= toStartOfDay(now() - INTERVAL 2 DAY)
GROUP BY chain_id, day, spoke, collateral_token, debt_token;

CREATE TABLE IF NOT EXISTS core.events_daily (
  chain_id UInt32,
  day Date,
  spoke LowCardinality(String),
  hub LowCardinality(String),
  name LowCardinality(String),
  token LowCardinality(String),
  n UInt64,
  users UInt64,
  usd Float64,
  refreshed_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(refreshed_at)
PARTITION BY (chain_id, toYYYYMM(day))
ORDER BY (chain_id, day, spoke, hub, name, token);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.events_daily_mv
REFRESH EVERY 5 MINUTE APPEND TO core.events_daily AS
SELECT chain_id, toDate(block_time) AS day, spoke, hub, name, token,
       count() AS n, uniqExact(user) AS users, sum(amount_usd) AS usd, now() AS refreshed_at
FROM core.events FINAL
WHERE block_time >= toStartOfDay(now() - INTERVAL 2 DAY)
GROUP BY chain_id, day, spoke, hub, name, token;

-- Supply APY, measured: growth of the share price (added assets per added share) over the last
-- 24 hours and 7 days, annualized over the time actually elapsed between the two readings.
-- NULL when there is no reading that old, or no shares.
CREATE VIEW IF NOT EXISTS core.hub_asset_apy AS
SELECT chain_id, hub, asset_id,
       argMax(share, block_number) AS share_price,
       max(block_time) AS read_at,
       argMaxIf(share, block_number, block_time <= now() - INTERVAL 1 DAY) AS share_1d,
       maxIf(block_time, block_time <= now() - INTERVAL 1 DAY) AS read_1d,
       argMaxIf(share, block_number, block_time <= now() - INTERVAL 7 DAY) AS share_7d,
       maxIf(block_time, block_time <= now() - INTERVAL 7 DAY) AS read_7d,
       if(share_1d > 0 AND read_at > read_1d, pow(share_price / share_1d, 31536000 / (toUnixTimestamp(read_at) - toUnixTimestamp(read_1d))) - 1, NULL) AS supply_apy_1d,
       if(share_7d > 0 AND read_at > read_7d, pow(share_price / share_7d, 31536000 / (toUnixTimestamp(read_at) - toUnixTimestamp(read_7d))) - 1, NULL) AS supply_apy_7d
FROM (
  SELECT chain_id, toString(hub) AS hub, asset_id, block_number, block_time, toFloat64(added_assets) / toFloat64(added_shares) AS share
  FROM core.hub_asset_state
  WHERE block_time >= now() - INTERVAL 9 DAY AND added_shares > 0
)
GROUP BY chain_id, hub, asset_id;
