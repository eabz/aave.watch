# Data pipeline

How chain data becomes what the API serves: the stages, the ClickHouse schema, the Dragonfly keys.
This file is the authority for storage; [architecture.md](architecture.md) describes the services
around it, [dashboard-data.md](dashboard-data.md) the numbers that come out.

## Principles

1. **Raw first.** Everything fetched from a node is stored as fetched, before any decoding. Parsed
   and enriched data are derived from raw and can be rebuilt without touching a node.
2. **On-chain only.** No number shown comes from a third-party API. Prices are the ones Aave's own
   oracles report.
3. **Multi-chain from row one.** `chain_id` is the first key of every table, key and stream.
4. **One process.** One indexer binary runs every chain and every schedule. No external cron.
5. **Each store does what it is good at.** ClickHouse holds settled history. Dragonfly holds the
   unsettled tip, the latest state and the streams.

## Chains

Aave v4 is live on four chains. Each is an entry in [registry/chains.toml](../registry/chains.toml),
not code: chain id, block time, the seed Hubs with their first block, and `settle_tag`.

| Chain | Id | Block time | Hubs | Spokes found | `settle_tag` | Behind the head |
|---|---|---|---|---|---|---|
| Ethereum | 1 | 12s | Core, Plus, Prime, Global Dollar | 55 | `finalized` | about 65 blocks, 13 min |
| Base | 8453 | 2s | Equities | 3 | `safe` | about 350 blocks, 12 min |
| Avalanche | 43114 | 1 to 2s | Core | 12 | `finalized` | about 7 blocks, seconds |
| Arc | 5042 | 0.5s | Core | 8 | `finalized` | 0 blocks |

Lags measured on public RPCs on 2026-11-18. RPC endpoints and the Multicall3 address are
deployment config (environment), not in the registry.

### Finality, and why a tag instead of a block count

A block is written to ClickHouse when the chain itself says it will not change. Each chain exposes
that as a block tag, so `settle_tag` names the tag rather than guessing a number of confirmations.

- **Ethereum.** `finalized` is two epochs back; reverting it needs a third of the stake slashed.
  `safe` (one epoch, about 6 min) would halve the lag and can be switched to in the registry.
- **Base.** Three stages: sealed by the sequencer every 2s (can still reorg), `safe` once its batch
  is on Ethereum, `finalized` once that Ethereum block is finalized. Base states that no block
  batched to Ethereum has ever been reorged, and `finalized` measured about 6,000 blocks (3.3 h)
  behind, so `safe` is the tag. Base plans native 200ms blocks ("Denim"); the tag still applies,
  only the block count behind the head grows.
- **Avalanche.** Snowman accepts a block in one to two seconds and an accepted block is final.
  `finalized` trails the head by a few blocks.
- **Arc.** Malachite (Tendermint-style BFT) with a permissioned validator set: a block is final
  when two thirds commit, in under a second. `finalized` equals the head.

So ClickHouse trails the head by about 13 minutes on Ethereum and Base and by seconds on Avalanche
and Arc. The gap is served from Dragonfly, where every block lands at once.

## Stages

```
            ┌──────────────── one indexer process, one task group per chain ───────────────┐
 node  ──▶  │ 1 fetch ──▶ 2 raw ──▶ 3 parse ──▶ 4 enrich ──▶ 5 sinks                       │
            │   heads        logs       events      USD, token     tip    ──▶ Dragonfly     │
            │   logs         calls      state       canonical      settled ──▶ ClickHouse   │
            │   calls        blocks                 asset                                   │
            │                                                                               │
            │ scheduler: which views to call at which block (events, heartbeat, risk tiers)  │
            └───────────────────────────────────────────────────────────────────────────────┘
```

1. **Fetch.** New heads over WebSocket; `eth_getLogs` for the watched addresses; view calls through
   Multicall3 at a fixed block.
2. **Raw.** The block header, each log (address, topics, data) and each call (target, calldata,
   returned bytes), exactly as returned.
3. **Parse.** Pure function of raw rows and the ABI: a log becomes an event row, a call result
   becomes a state row. No I/O. Versioned (`parser_version`).
4. **Enrich.** Adds what needs other rows: USD value at that block from the oracle price, the
   token, the canonical asset. Names and logos are *not* written into rows; they are joined at
   read time from dictionaries, so a rename never rewrites history.
5. **Sinks.** Every block goes to Dragonfly at once, marked unsettled. Once a block is
   at or below the chain's `settle_tag` it is written to ClickHouse: raw, parsed and enriched in one
   batch per table, then the cursor.

### Three ways to run the same code

| Mode | Source | Writes |
|---|---|---|
| `live` | node, at the head | Dragonfly tip, then ClickHouse on settle |
| `backfill` | node, block ranges from each contract's deployment block | ClickHouse only |
| `reparse` | ClickHouse `raw.*` | ClickHouse `core.*` only. No node. |

Backfill happens once per chain. After that, a parser bug, a new column or a new enrichment is a
`reparse` of the affected range: drop the `core` partitions, replay raw through stages 3 to 5.

Not everything in `raw` is parsed. `Transfer` and `Approval` logs of the tokenization Spokes stay in
`raw.logs` only (decided 2026-11-18): they are vault share moves, not lending activity. Their
`Deposit` and `Withdraw` logs are parsed.

What reparse cannot give: a view we never called. Events are complete forever (every log of every
watched contract is in `raw.logs`), but state exists only for the blocks where we called it. A new
kind of state read needs its calls backfilled from an archive node. To keep that rare, the first
backfill calls the whole struct getters (`getAsset`, `getSpoke`, `getReserve`, `getUserPosition`,
`getUserAccountData`), not field getters.

### Backfill order

Spokes are discovered from Hub events, so each chain backfills in two passes of one run:

1. Hubs, from their deployment block: learns every asset, every Spoke and the block each was added.
2. Spokes and their oracles, each from the block it was added.

State calls are made at the blocks the scheduler rules below select, as the range is walked.

### Scheduler (inside the indexer)

| State | Read when |
|---|---|
| Hub asset, Hub asset × Spoke | the block had an event for it, plus an hourly heartbeat |
| Spoke reserve | the block had an event for it, plus hourly |
| Oracle prices | every block live (one multicall per chain); during backfill on blocks with events plus hourly |
| User position | the block had an event for that user |
| User health factor | by tier: HF < 1.1 every block, < 1.5 every 5 min, the rest hourly |

The schedule state (who is due when) lives in a Dragonfly sorted set, so a restart resumes it; it
can be rebuilt from ClickHouse. Timers are tokio intervals keyed on block numbers, not wall-clock
cron.

Rollups are ClickHouse refreshable materialized views (below). With those two, nothing outside the
indexer and ClickHouse runs on a schedule.

### Reorgs

Only settled blocks reach ClickHouse, so ClickHouse is never rewritten in normal operation. The
unsettled tip lives in Dragonfly, where replacing a block is cheap:

1. A new head whose parent hash does not match: find the fork block.
2. Publish `reorg {from_block}` on `chain:{chain_id}`. Clients drop unsettled items from that block.
3. Re-fetch from the fork block and publish again.

A settled block whose hash later changes is an incident, not a case to handle: the indexer stops
that chain and logs it. The fix is to delete rows from the fork block up in `raw` and `core` and
backfill that range. `raw.blocks` keeps hashes so this is detectable.

### Restart

On start, per chain: read the cursor, delete rows above it in every table (a crash can land between
a batch and the cursor), then resume from cursor + 1. Unsettled blocks are simply fetched again.

## Prices

Each Spoke has one `AaveOracle`. Each reserve has a source that answers `latestAnswer()` and
`decimals()` (Chainlink's interface, whatever sits behind it).

- **Price of record:** `AaveOracle.getReservesPrices` at the block. It is the price the protocol
  liquidates with, so it is the price we show.
- **Source detail:** `getReserveSource` gives the feed address. Where the feed is a Chainlink
  aggregator we also read `latestRoundData()` for `updatedAt`; that is what "stale price" means.
- **One price per Hub asset:** the median of the Spokes' prices for that asset on that chain
  (still the open decision in dashboard-data.md).
- **No external price source.** A reserve cannot be listed without an on-chain source, so there is
  nothing for CoinGecko to fill. If we later show a token Aave does not price, that number is
  labelled as off-chain and stays out of every risk figure.

## Tokens and metadata

| Data | Source | Stored in |
|---|---|---|
| `symbol`, `name`, `decimals` | the token contract, one multicall when first seen | `meta.tokens` |
| Hub, Spoke display names | not on chain (a Hub has no name field) | `registry/chains.toml` (Hubs, done); a Spoke names file there, to add |
| Logo | not on chain | `apps/web/public/tokens/{chain_id}/{address}.svg` in this repo, served by the Worker |
| Canonical asset (USDC on four chains is one asset) | our mapping | `registry/assets.toml` |
| Address labels (liquidators, treasuries) | our list | `registry/labels.toml` |

Logos and names are files we keep, loaded into ClickHouse at start-up. That is curation, not a
runtime dependency: no request leaves the server to render a page. A token with no entry shows its
on-chain symbol and a generic mark. `symbol()` that returns `bytes32` is handled at decode.

## ClickHouse

One server. Three databases:

| Database | Holds | Rebuilt from |
|---|---|---|
| `raw` | what nodes returned | a node (expensive, once) |
| `core` | parsed and enriched facts, latest-state tables, rollups | `raw` (cheap) |
| `meta` | registry, tokens, labels, cursor, dictionaries | chain + repo files |

Conventions for every table:

- `chain_id UInt32` first in `ORDER BY`. `PARTITION BY (chain_id, toYYYYMM(block_time))` on fact
  tables, so one chain-month can be dropped and reparsed.
- Addresses and hashes are lowercase hex `String` (`LowCardinality` for contracts). Simple to
  query by hand; ZSTD makes the cost small.
- Token amounts and shares are `UInt256`, raw units. USD is `Float64`. Rates and indexes keep the
  contract's scale (`UInt128`, ray) with a `Float64` copy where charts need it.
- `ReplacingMergeTree(inserted_at)` on the natural key, so writing a block twice is harmless.
- `CODEC(ZSTD(3))` on wide string columns, `Delta, ZSTD` on block numbers and times.

### raw

```sql
CREATE TABLE raw.blocks (
  chain_id UInt32, number UInt64, hash String, parent_hash String,
  block_time DateTime('UTC'), inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time)) ORDER BY (chain_id, number);

CREATE TABLE raw.logs (
  chain_id UInt32, block_number UInt64, block_time DateTime('UTC'), block_hash String,
  tx_hash String, tx_index UInt32, log_index UInt32,
  address LowCardinality(String),
  topic0 LowCardinality(String), topic1 String, topic2 String, topic3 String,
  data String CODEC(ZSTD(3)),
  inserted_at DateTime DEFAULT now(),
  INDEX ix_tx tx_hash TYPE bloom_filter GRANULARITY 4
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, address, topic0, block_number, log_index);

CREATE TABLE raw.calls (
  chain_id UInt32, block_number UInt64, block_time DateTime('UTC'),
  target LowCardinality(String), selector LowCardinality(String),
  args String,                        -- calldata after the selector
  ok UInt8, result String CODEC(ZSTD(3)),
  reason LowCardinality(String),      -- why it was read: event, heartbeat, tier, backfill
  inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, target, selector, args, block_number);
```

`raw.logs` is ordered by contract and event, because reparse reads "all `Borrow` logs of this
Spoke", not "block N". `raw.calls` is the large table: user reads dominate it.

### core: events

One wide table for every decoded event.

```sql
CREATE TABLE core.events (
  chain_id UInt32, block_number UInt64, block_time DateTime('UTC'), log_index UInt32,
  tx_hash String,
  contract LowCardinality(String),
  kind LowCardinality(String),        -- hub, spoke, oracle, ir_strategy
  name LowCardinality(String),        -- Borrow, LiquidationCall, UpdateSpokeConfig, ...
  hub LowCardinality(String), spoke LowCardinality(String),
  asset_id Nullable(UInt32), reserve_id Nullable(UInt32),
  user String, caller String,
  amount UInt256, shares UInt256,
  token LowCardinality(String),       -- enrich
  amount_usd Float64, price Float64,  -- enrich: oracle price at this block
  args String CODEC(ZSTD(3)),         -- every decoded argument, JSON
  parser_version UInt16, inserted_at DateTime DEFAULT now(),
  INDEX ix_tx tx_hash TYPE bloom_filter GRANULARITY 4,
  PROJECTION by_user  (SELECT * ORDER BY chain_id, user, block_number, log_index),
  PROJECTION by_place (SELECT * ORDER BY chain_id, spoke, hub, name, block_number, log_index)
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY (chain_id, toYYYYMM(block_time))
ORDER BY (chain_id, block_number, log_index)
SETTINGS deduplicate_merge_projection_mode = 'rebuild';
```

Three read paths, three orders: the feed and "what happened in block N" use the main key; the
wallet page uses `by_user`; Spoke, Hub and "all liquidations" pages use `by_place`.

### core: state

One table per kind of read. Each row is one read at one block.

| Table | Order by (after `chain_id`) | From call |
|---|---|---|
| `hub_asset_state` | `hub, asset_id, block_number` | `Hub.getAsset`, `getAddedAssets`, `getAssetTotalOwed` |
| `hub_spoke_state` | `hub, asset_id, spoke, block_number` | `Hub.getSpoke`, `getSpokeAddedAssets`, `getSpokeTotalOwed` |
| `reserve_state` | `spoke, reserve_id, block_number` | `Spoke.getReserve`, `getReserveSuppliedAssets`, `getReserveTotalDebt` |
| `prices` | `spoke, reserve_id, block_number` | `AaveOracle.getReservesPrices`; a row only when the price changed |
| `asset_prices` | `hub, asset_id, block_number` | derived: one price per Hub asset |
| `user_reserve_state` | `spoke, user, reserve_id, block_number` | `Spoke.getUserPosition`, `getUserSuppliedAssets`, `getUserTotalDebt` |
| `user_account_state` | `spoke, user, block_number` | `Spoke.getUserAccountData` |

Each state row carries its amounts in raw units, its USD values from enrich, `reason`, and
`parser_version`.

**Latest tables.** "The current row for every X" is the hottest query shape and a bad one for a
history table. Each state table has a twin `*_latest`, `ReplacingMergeTree(block_number)` ordered
by the entity alone (for example `user_account_latest` on `(chain_id, spoke, user)`), fed by an
ordinary materialized view. Replacing is idempotent, so double inserts cannot corrupt it. Queries
read it with `FINAL`. The health factor histogram, top positions and concentration panels read
these.

### core: rollups

Refreshable materialized views, recomputed from `core` on a timer inside ClickHouse:

```sql
CREATE MATERIALIZED VIEW core.events_hourly
REFRESH EVERY 1 MINUTE TO core.events_hourly_data AS
SELECT chain_id, toStartOfHour(block_time) AS hour, spoke, hub, name, token,
       count() AS n, uniqExact(user) AS users, sum(amount_usd) AS usd
FROM core.events FINAL
WHERE block_time >= now() - INTERVAL 3 DAY
GROUP BY ALL;
```

The target table is a `ReplacingMergeTree` on the group key, so each refresh overwrites the recent
buckets and leaves older ones alone. Chosen over incremental (insert-triggered) views because those
double-count when a block is written twice and cannot follow a reparse. A reparse of old data
re-runs the same `SELECT` once without the time filter.

First rollups: `events_hourly`, `events_daily`, `hub_asset_hourly` (last state per hour, for supply,
debt, usage and rate charts), `hub_spoke_hourly`, `reserve_hourly`, `liquidations_daily`. More are
added when a panel needs one, not before.

### meta

| Table | Key | Content |
|---|---|---|
| `chains` | `chain_id` | name, block time, settle tag, explorer URL |
| `hubs`, `spokes` | `chain_id, address` | name, kind (spoke, treasury, tokenization, babylon), first block, oracle |
| `assets` | `chain_id, hub, asset_id` | underlying token, decimals, first block |
| `reserves` | `chain_id, spoke, reserve_id` | hub, asset_id, token, first block |
| `tokens` | `chain_id, address` | symbol, name, decimals, canonical asset, logo path |
| `price_sources` | `chain_id, spoke, reserve_id` | feed address, feed kind, decimals, since block |
| `labels` | `chain_id, address` | label, category |
| `cursor` | `chain_id, stream` | last settled block and hash, per mode |
| `parser_runs` | `chain_id, parser_version, range` | when a reparse ran and over what |

All `ReplacingMergeTree`. `tokens`, `hubs`, `spokes`, `reserves` and `labels` are also exposed as
ClickHouse dictionaries, so queries use `dictGet` for names instead of joins.

### How the API reads

| Question | Table and key used |
|---|---|
| Events of a wallet | `core.events`, projection `by_user` |
| Events of a Spoke or Hub, by type | `core.events`, projection `by_place` |
| A transaction | `core.events`, bloom index on `tx_hash` |
| Supply, debt, rate over time | `hub_asset_hourly`, `reserve_hourly` |
| Hub-to-Spoke map and caps now | `hub_spoke_state_latest` |
| HF distribution, top positions | `user_account_latest` |
| History of one position | `user_account_state` prefix `(chain_id, spoke, user)` |
| Price shock | `user_reserve_latest` joined to `prices_latest` and reserve config |
| Config timeline | `core.events` where `name` is a config event, `by_place` |
| Anything "right now" | Dragonfly first (below) |

### Migrations

The schema is the numbered `.sql` files in `infra/clickhouse/`. `./infra/clickhouse/migrate.sh`
applies each once, in order, and records it in `meta.schema_migrations`. Arguments pass through to
`clickhouse-client`. A change to the schema is a new numbered file, never an edit to an applied one.

### Size

Not a constraint for now. The largest table will be `raw.calls` (user reads). If disk ever matters,
the levers are a `TTL` on tier reads in `raw.calls` and thinning old user reads to daily.

## Dragonfly

Holds what is small, hot and replaceable. Losing it loses nothing: it refills from the node (tip)
and from ClickHouse (`*_latest`). Every key starts with the chain id.

### Streams (pub/sub with replay)

Streams trimmed by age (`XADD ... MINID ~`), read by the API with `XREAD BLOCK`, relayed to
browsers as SSE. One hour is longer than any chain takes to settle a block, so the tip is always
whole. Names are defined once, in `crates/protocol/src/keys.rs`.

| Stream | Entry | Kept |
|---|---|---|
| `feed:{chain_id}` | one decoded, enriched event | 1 hour |
| `state:{chain_id}` | one changed state row: hub asset, hub × spoke, reserve, price | 1 hour |
| `risk:{chain_id}` | a position crossing a health band, or a liquidation | 1 hour |
| `chain:{chain_id}` | `head`, `settled`, `reorg` | 1 hour |

Every entry is one JSON field with the same envelope:

```json
{ "v": 1, "chain_id": 1, "block": 21480112, "block_hash": "0x…", "log_index": 14,
  "settled": false, "type": "event", "name": "Borrow", "data": { } }
```

A client that reconnects sends the last stream id it saw and receives what it missed. An item
arrives once as unsettled; `chain` later says which block settled. `reorg {from_block}` tells
clients to drop unsettled items from that block.

Streams rather than `PUBLISH`: plain pub/sub loses whatever is sent while a reader is away, and the
API would need its own buffer to offer replay.

### Latest state

| Key | Type | Content |
|---|---|---|
| `head:{chain_id}` | hash | number, hash, time, settled number |
| `hub:{chain_id}:{hub}:{asset_id}` | string (JSON) | latest hub asset row |
| `hubspoke:{chain_id}:{hub}:{asset_id}:{spoke}` | string (JSON) | caps, usage, flags |
| `reserve:{chain_id}:{spoke}:{reserve_id}` | string (JSON) | latest reserve row |
| `price:{chain_id}:{spoke}` | hash | reserve_id → price, block |
| `user:{chain_id}:{spoke}:{user}` | hash | hf, collateral, debt, risk premium, block |

### Indexes for the live risk view

| Key | Type | Score → member | Serves |
|---|---|---|---|
| `hf:{chain_id}:{spoke}` | sorted set | health factor → user | positions at risk, `ZRANGEBYSCORE 0 1.1` |
| `debt:{chain_id}:{spoke}` | sorted set | debt USD → user | top positions |
| `due:{chain_id}` | sorted set | next read block → `spoke:user` | the scheduler's queue |

The at-risk table, the product's main object, is a sorted-set range plus a pipeline of `HGETALL`:
no ClickHouse query on the hot path.

### Cache and control

| Key | Type | Use |
|---|---|---|
| `q:{hash}` | string, TTL | cached ClickHouse result for a heavy endpoint |
| `lock:indexer` | string, TTL | one indexer at a time |

## What goes where

| Need | ClickHouse | Dragonfly |
|---|---|---|
| History, any range, any grouping | yes | no |
| Scans over millions of rows, aggregates, joins | yes | no |
| Source of truth, replay, audit by block | yes (`raw`) | no |
| The last few blocks before they settle | no | yes |
| "Now" for one entity, in under a millisecond | no | yes |
| Ranked live sets (lowest HF, largest debt) | on `*_latest`, seconds old | yes, block-fresh |
| Push to clients with replay | no | yes (streams) |
| Work queue and locks for the indexer | no | yes |
| Survives loss of the other store | yes | rebuilt from ClickHouse and the node |

## Postgres?

No, not now. Nothing in this design is transactional or edited by users: facts are append-only,
the registry is a few hundred rows that ClickHouse dictionaries serve well, and the cursor is one
row per chain. A third database would be another service to run, back up and keep in step, for
tables that fit in memory.

It becomes worth it the day there is data that users own and change: accounts, saved alerts,
watchlists, API keys, labels edited in the UI. That is row-level updates with constraints, which
ClickHouse is bad at. On that day the choice is Postgres on the server or Cloudflare D1 next to the
Worker; D1 is the smaller step, since those reads come from the edge anyway.

## Additions this design leaves room for

| Addition | What it needs | New backfill? |
|---|---|---|
| A new chain | a config entry and its registry files | that chain only |
| A new event or a renamed field | parser change, `reparse` | no |
| A new derived metric | a rollup or a column, `reparse` | no |
| A new view call | scheduler rule, parser | yes, calls only, from an archive node |
| Gas and liquidator profit | `raw.receipts` for our transactions (fetched by hash) | receipts only |
| Exact price-change blocks | watch the Chainlink aggregators' `AnswerUpdated` logs | logs of those feeds |
| Governance and access changes | add the `AccessManager` and configurator addresses to the watch list | logs of those contracts |
| Tokenization Spoke share moves (`Transfer`, `Approval`) | parser rules; kept in `raw` only for now, by decision | no |
| Babylon Spoke | its ABI (none deployed yet) | no, once watched |
| Alerts | a consumer of `risk:{chain_id}` and `state:{chain_id}` | no |
| User accounts, saved views | D1 or Postgres (above) | no |

To make the "no" answers true, the watch list for `raw.logs` from day one is every Hub, every Spoke
of every kind, every Spoke's `AaveOracle`, every interest rate strategy and the access manager, with
every log kept, including events the parser does not know yet. Oracles are in
`registry/spokes.toml`; strategies and the access manager are read from the Hubs (`getAssetConfig`,
`authority`).

## Open decisions

1. Ethereum on `finalized` (13 min) or `safe` (6 min).
2. Our own nodes or a provider. "On-chain only" holds either way; a provider is still a dependency
   for availability. Backfill needs archive state on all four chains.
3. Whether tier reads below HF 1.1 on every block are affordable on Arc's half-second blocks and
   Base's coming 200ms blocks. Likely answer: read every N blocks there.
