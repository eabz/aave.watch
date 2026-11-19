-- Hub discovery (docs/data-pipeline.md, "Hub discovery"): every contract seen to emit AddAsset
-- that is not a seed Hub, and the decision on it. admitted = 1 rows are also in meta.hubs;
-- admitted = 0 rows are not synced and not shown, and are looked at again every ten minutes.
CREATE TABLE IF NOT EXISTS meta.hub_candidates (
  chain_id UInt32,
  address String,
  first_block UInt64,  -- block of the first AddAsset seen
  authority String,    -- authority() when last checked, empty if it has none
  admitted UInt8,
  reason String,       -- why it was admitted, or why not
  updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at) ORDER BY (chain_id, address);
