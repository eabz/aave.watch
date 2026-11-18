-- Names are joined at read time with dictGet, so a rename never rewrites history.
CREATE DICTIONARY IF NOT EXISTS meta.tokens_dict (
  chain_id UInt32,
  address String,
  symbol String,
  name String,
  decimals UInt8,
  canonical String,
  logo String
) PRIMARY KEY chain_id, address
SOURCE(CLICKHOUSE(QUERY 'SELECT chain_id, address, symbol, name, decimals, canonical, logo FROM meta.tokens FINAL'))
LAYOUT(COMPLEX_KEY_HASHED()) LIFETIME(MIN 30 MAX 60);

CREATE DICTIONARY IF NOT EXISTS meta.hubs_dict (
  chain_id UInt32,
  address String,
  name String
) PRIMARY KEY chain_id, address
SOURCE(CLICKHOUSE(QUERY 'SELECT chain_id, address, name FROM meta.hubs FINAL'))
LAYOUT(COMPLEX_KEY_HASHED()) LIFETIME(MIN 30 MAX 60);

CREATE DICTIONARY IF NOT EXISTS meta.spokes_dict (
  chain_id UInt32,
  address String,
  name String,
  kind String
) PRIMARY KEY chain_id, address
SOURCE(CLICKHOUSE(QUERY 'SELECT chain_id, address, name, kind FROM meta.spokes FINAL'))
LAYOUT(COMPLEX_KEY_HASHED()) LIFETIME(MIN 30 MAX 60);

CREATE DICTIONARY IF NOT EXISTS meta.labels_dict (
  chain_id UInt32,
  address String,
  label String,
  category String
) PRIMARY KEY chain_id, address
SOURCE(CLICKHOUSE(QUERY 'SELECT chain_id, address, label, category FROM meta.labels FINAL'))
LAYOUT(COMPLEX_KEY_HASHED()) LIFETIME(MIN 30 MAX 60);
