-- Aave v3 totals, read hourly next to v4. Spec: docs/data-pipeline.md, "Aave v3 totals".

-- One view-call answer per row, as returned. A row with market = '' marks an hour as read.
CREATE TABLE IF NOT EXISTS raw.v3_calls (
  chain_id UInt32,
  market LowCardinality(String),
  block_number UInt64 CODEC(Delta, ZSTD),
  block_time DateTime('UTC') CODEC(Delta, ZSTD),
  target LowCardinality(String),
  selector LowCardinality(String),
  args String,
  ok UInt8,
  result String CODEC(ZSTD(3)),
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY toYYYYMM(block_time)
ORDER BY (chain_id, market, block_number, target, selector, args);

-- One row per reserve per reading. Every answer used here is a single uint256.
-- Selectors: 0x51460e25 getATokenTotalSupply, 0x4d44ac4f getTotalDebt, 0xb3596f07 getAssetPrice,
-- 0x313ce567 decimals (asked of the token itself), 0x8c89b64f BASE_CURRENCY_UNIT.
-- shortcut: decoded from raw on every query; make it a table filled by a materialized view when
-- raw.v3_calls passes about 20M rows.
CREATE VIEW IF NOT EXISTS core.v3_reserve_state AS
SELECT chain_id, market, block_number, block_time, asset,
       supplied_raw / exp10(decimals) AS supplied, borrowed_raw / exp10(decimals) AS borrowed,
       price_raw / base_unit AS price,
       supplied * price AS supplied_usd, borrowed * price AS borrowed_usd
FROM (
  SELECT chain_id, market, block_number, any(block_time) AS block_time,
         multiIf(selector = '0x8c89b64f', '', selector = '0x313ce567', target, concat('0x', right(args, 40))) AS asset,
         maxIf(v, selector = '0x51460e25') AS supplied_raw, maxIf(v, selector = '0x4d44ac4f') AS borrowed_raw,
         maxIf(v, selector = '0xb3596f07') AS price_raw, maxIf(v, selector = '0x313ce567') AS decimals,
         max(maxIf(v, selector = '0x8c89b64f')) OVER (PARTITION BY chain_id, market, block_number) AS base_unit
  FROM (
    SELECT chain_id, market, block_number, block_time, target, selector, args,
           toFloat64(reinterpretAsUInt256(reverse(unhex(substring(result, 3, 64))))) AS v
    FROM raw.v3_calls
    WHERE ok = 1 AND length(result) >= 66
      AND selector IN ('0x51460e25', '0x4d44ac4f', '0xb3596f07', '0x313ce567', '0x8c89b64f')
  )
  GROUP BY chain_id, market, block_number, asset
)
WHERE asset != '' AND base_unit > 0;
