-- Rollups are refreshable views: ClickHouse recomputes the recent buckets on a timer and appends
-- them; the ReplacingMergeTree target keeps the newest version of each bucket. Not insert-triggered,
-- so a block written twice or a reparse cannot double count. No external cron.
-- After a reparse of old data, run each SELECT once without the time filter (INSERT INTO ... SELECT).

CREATE TABLE IF NOT EXISTS core.events_hourly (
  chain_id UInt32,
  hour DateTime('UTC'),
  spoke LowCardinality(String),
  hub LowCardinality(String),
  name LowCardinality(String),
  token LowCardinality(String),
  n UInt64,
  users UInt64,
  usd Float64,
  refreshed_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(refreshed_at)
PARTITION BY (chain_id, toYYYYMM(hour))
ORDER BY (chain_id, hour, spoke, hub, name, token);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.events_hourly_mv
REFRESH EVERY 1 MINUTE APPEND TO core.events_hourly AS
SELECT chain_id, toStartOfHour(block_time) AS hour, spoke, hub, name, token,
       count() AS n, uniqExact(user) AS users, sum(amount_usd) AS usd, now() AS refreshed_at
FROM core.events FINAL
WHERE block_time >= toStartOfHour(now() - INTERVAL 3 HOUR)
GROUP BY chain_id, hour, spoke, hub, name, token;

-- Last read of each hour: supply, debt, usage and rate charts.
CREATE TABLE IF NOT EXISTS core.hub_asset_hourly (
  chain_id UInt32,
  hub LowCardinality(String),
  asset_id UInt32,
  hour DateTime('UTC'),
  block_number UInt64,
  added_assets UInt256,
  drawn_owed UInt256,
  premium_owed UInt256,
  liquidity UInt256,
  swept UInt256,
  deficit_ray UInt256,
  usage Float64,
  drawn_rate_apr Float64,
  price Float64,
  added_usd Float64,
  owed_usd Float64,
  liquidity_usd Float64,
  refreshed_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(refreshed_at)
PARTITION BY (chain_id, toYYYYMM(hour))
ORDER BY (chain_id, hub, asset_id, hour);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.hub_asset_hourly_mv
REFRESH EVERY 1 MINUTE APPEND TO core.hub_asset_hourly AS
SELECT chain_id, hub, asset_id, toStartOfHour(block_time) AS hour,
       max(s.block_number) AS block_number,
       argMax(added_assets, s.block_number) AS added_assets,
       argMax(drawn_owed, s.block_number) AS drawn_owed,
       argMax(premium_owed, s.block_number) AS premium_owed,
       argMax(liquidity, s.block_number) AS liquidity,
       argMax(swept, s.block_number) AS swept,
       argMax(deficit_ray, s.block_number) AS deficit_ray,
       argMax(usage, s.block_number) AS usage,
       argMax(drawn_rate_apr, s.block_number) AS drawn_rate_apr,
       argMax(price, s.block_number) AS price,
       argMax(added_usd, s.block_number) AS added_usd,
       argMax(owed_usd, s.block_number) AS owed_usd,
       argMax(liquidity_usd, s.block_number) AS liquidity_usd,
       now() AS refreshed_at
FROM core.hub_asset_state AS s
WHERE block_time >= toStartOfHour(now() - INTERVAL 3 HOUR)
GROUP BY chain_id, hub, asset_id, hour;

CREATE TABLE IF NOT EXISTS core.hub_spoke_hourly (
  chain_id UInt32,
  hub LowCardinality(String),
  asset_id UInt32,
  spoke LowCardinality(String),
  hour DateTime('UTC'),
  block_number UInt64,
  added_assets UInt256,
  drawn_owed UInt256,
  premium_owed UInt256,
  add_cap UInt64,
  draw_cap UInt64,
  added_usd Float64,
  owed_usd Float64,
  refreshed_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(refreshed_at)
PARTITION BY (chain_id, toYYYYMM(hour))
ORDER BY (chain_id, hub, asset_id, spoke, hour);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.hub_spoke_hourly_mv
REFRESH EVERY 1 MINUTE APPEND TO core.hub_spoke_hourly AS
SELECT chain_id, hub, asset_id, spoke, toStartOfHour(block_time) AS hour,
       max(s.block_number) AS block_number,
       argMax(added_assets, s.block_number) AS added_assets,
       argMax(drawn_owed, s.block_number) AS drawn_owed,
       argMax(premium_owed, s.block_number) AS premium_owed,
       argMax(add_cap, s.block_number) AS add_cap,
       argMax(draw_cap, s.block_number) AS draw_cap,
       argMax(added_usd, s.block_number) AS added_usd,
       argMax(owed_usd, s.block_number) AS owed_usd,
       now() AS refreshed_at
FROM core.hub_spoke_state AS s
WHERE block_time >= toStartOfHour(now() - INTERVAL 3 HOUR)
GROUP BY chain_id, hub, asset_id, spoke, hour;

CREATE TABLE IF NOT EXISTS core.reserve_hourly (
  chain_id UInt32,
  spoke LowCardinality(String),
  reserve_id UInt32,
  hour DateTime('UTC'),
  block_number UInt64,
  supplied_assets UInt256,
  drawn_debt UInt256,
  premium_debt UInt256,
  price Float64,
  supplied_usd Float64,
  debt_usd Float64,
  refreshed_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(refreshed_at)
PARTITION BY (chain_id, toYYYYMM(hour))
ORDER BY (chain_id, spoke, reserve_id, hour);

CREATE MATERIALIZED VIEW IF NOT EXISTS core.reserve_hourly_mv
REFRESH EVERY 1 MINUTE APPEND TO core.reserve_hourly AS
SELECT chain_id, spoke, reserve_id, toStartOfHour(block_time) AS hour,
       max(s.block_number) AS block_number,
       argMax(supplied_assets, s.block_number) AS supplied_assets,
       argMax(drawn_debt, s.block_number) AS drawn_debt,
       argMax(premium_debt, s.block_number) AS premium_debt,
       argMax(price, s.block_number) AS price,
       argMax(supplied_usd, s.block_number) AS supplied_usd,
       argMax(debt_usd, s.block_number) AS debt_usd,
       now() AS refreshed_at
FROM core.reserve_state AS s
WHERE block_time >= toStartOfHour(now() - INTERVAL 3 HOUR)
GROUP BY chain_id, spoke, reserve_id, hour;
