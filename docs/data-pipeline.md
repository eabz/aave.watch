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
4. **Fetch once.** Only the fetcher talks to a node, and it asks for each log and each call one
   time. Everything downstream reads what the fetcher stored. One `indexer` binary, two processes
   (`indexer fetch`, `indexer process`), every chain and every schedule inside them. No external
   cron.
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

Lags measured on public RPCs on 2026-11-18. RPC endpoints are deployment config (environment),
not in the registry.

**Multicall.** Multicall3 is not deployed at its usual address on these chains, and Multicall2 only
on Ethereum. So no deployment is relied on: every `eth_call` carries Multicall2's runtime bytecode
as a state override at a fixed address (`crates/protocol/abi/multicall2.hex`, `codec::MULTICALL`).
Checked 2026-11-18 on all four chains, at the head and at old blocks.

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
            ┌──── indexer fetch ─────┐                  ┌────────── indexer process ──────────┐
 node  ──▶  │ 1 fetch ──▶ 2 raw      │ ── raw stream ─▶ │ 3 parse ──▶ 4 enrich ──▶ 5 sinks    │
            │   heads        logs    │    (Dragonfly)   │   events      USD, token   tip     ──▶ Dragonfly
            │   logs         calls   │ ── raw tables ─▶ │   state       canonical    settled ──▶ ClickHouse
            │   calls        blocks  │    (ClickHouse)  │               asset                 │
            │                        │                  │                                     │
            │ scheduler: which views │                  │ no node access                      │
            │ to call at which block │                  │                                     │
            └────────────────────────┘                  └─────────────────────────────────────┘
```

Two processes, one task group per chain in each. The fetcher is the only one with an RPC URL. The
processor can be stopped, fixed and restarted without dropping a node connection or asking a node
for anything twice; it resumes from its own cursor over raw.

1. **Fetch.** Logs for the watched addresses and view calls through a multicall at a fixed block.
   How they are fetched depends on how far behind the chain is: see
   [From backfill to live](#from-backfill-to-live).
2. **Raw.** The block header, each log (address, topics, data) and each call (target, calldata,
   returned bytes), exactly as returned. Settled blocks go to ClickHouse `raw.*`; unsettled blocks
   go to the Dragonfly stream `raw:{chain_id}` and are copied to ClickHouse when they settle, not
   fetched again.
3. **Parse.** Pure function of raw rows and the ABI: a log becomes an event row, a call result
   becomes a state row. No I/O. Versioned (`parser_version`).
4. **Enrich.** Adds what needs other rows: USD value at that block from the oracle price, the
   token, the canonical asset. Names and logos are *not* written into rows; they are joined at
   read time from dictionaries, so a rename never rewrites history.
5. **Sinks.** An unsettled block goes to Dragonfly at once, marked unsettled. Once the fetcher
   reports a block settled, the processor writes its parsed and enriched rows to ClickHouse, one
   batch per table, then its cursor.

### One code path, three sources

The processor runs stages 3 to 5 over raw rows and does not care where they came from.

| Processor reads | When | Writes |
|---|---|---|
| ClickHouse `raw.*`, from its cursor | the fetcher is `catching_up` | ClickHouse `core.*` |
| Dragonfly `raw:{chain_id}` | the fetcher is live | Dragonfly tip, then ClickHouse `core.*` on settle |
| ClickHouse `raw.*`, a chosen range (`reparse`) | a parser bug, a new column, a new enrichment | ClickHouse `core.*` |

Backfill happens once per chain. After that, a change to parsing is a `reparse` of the affected
range: drop the `core` partitions, replay raw through stages 3 to 5. No node is involved.

Every log an ABI knows is parsed, including the `Transfer` and `Approval` logs of the tokenization
Spokes (vault share moves; since parser version 2, which reversed the earlier decision to keep them
in `raw.logs` only) and the `Upgraded` and `AdminChanged` logs of the proxies.

What reparse cannot give: a view we never called. Events are complete forever (every log of every
watched contract is in `raw.logs`), but state exists only for the blocks where we called it. A new
kind of state read needs its calls backfilled from an archive node. To keep that rare, the first
backfill calls the whole struct getters (`getAsset`, `getSpoke`, `getReserve`, `getUserPosition`,
`getUserAccountData`), not field getters.

### Backfill order

One pass per chain, from its `start_block` in the registry: the block the earliest watched contract
was deployed in (the Treasury Spoke, a few blocks before the first Hub). The watch list is the Hubs plus every
Spoke and oracle known at that moment; asking for an address before it existed returns nothing and
costs nothing.

State calls are made at the blocks the scheduler rules below select, as the range is walked.

### Spoke discovery

The Spoke list is not static. `registry/spokes.toml` is a seed: it saves requests and carries the
names and kinds, but the fetcher does not depend on it.

- A Hub announces a Spoke with `AddSpoke(assetId, spoke)`. When a range contains one for an address
  that is not watched, that range is not stored. The fetcher reads the Spoke's `ORACLE()`, fetches
  the logs of both addresses from the chain's `start_block` up to the cursor (a Spoke is deployed
  and configured before a Hub announces it), adds them to the watch list and walks the range again.
- On start the watch list is the Hubs, the seed, and every Spoke named by an `AddSpoke` log already
  in `raw.logs`. Nothing else is persisted.
- Checked 2026-11-18 on Arc: a run with the seed ignored (`FETCH_NO_SEED=1`) found all 8 Spokes
  and stored the same 56,683 logs, by checksum, as the seeded run.

Interest rate strategies and access managers are discovered the same way: a Hub names its
strategies in `UpdateAssetConfig`, every Hub and Spoke names its access manager in
`AuthorityUpdated`. Their history is fetched once, and that is recorded in `meta.cursor`
(`stream = 'watch:{address}'`) so a contract with no logs is not walked again.

Price sources are not watched for logs: a Chainlink proxy emits none itself, and more than half of
the sources are adapters that are not Chainlink feeds. Their update time is a view call instead
(see "Prices"). Not discovered yet: a later Spoke's logs from before its chain's `start_block`.

### From backfill to live

The fetcher is a state machine per chain with two states. It moves between them by itself, in both
directions, so the first backfill and a reconnect after an outage are the same code.

| State | Transport | Shape |
|---|---|---|
| `catching_up` | HTTP, JSON-RPC batches | several block ranges in parallel, committed in order |
| `live` | one WebSocket | `eth_subscribe` to `newHeads` and to `logs` for the watch list; calls go over the same socket |

**`catching_up`.** Walk from the raw cursor to the chain's settled block in ranges. One
`eth_getLogs` per range carries every watched address. The range size adapts: halve it when the
node refuses the result size, grow it when a range comes back small. Ranges are fetched in parallel
but written in block order, so the raw cursor always means "every block at or below this is
complete". When the walk reaches the settled block it reads the tag again and continues; each round
is shorter than the last.

**The switch.** When the cursor is within one range of the settled block:

1. Open the WebSocket and subscribe. Buffer what arrives; do not process it. Call the first head
   received `H`.
2. Fetch `cursor + 1` to `H - 1` over HTTP, the same way as any other range. Blocks at or below the
   settled block go to ClickHouse `raw.*`; the rest go to `raw:{chain_id}`.
3. Seed the live schedule: one multicall at `H - 1` reads `getUserAccountData` for every user with
   debt, which puts each user in a health factor tier. The user list comes from the `core`
   latest-state tables, so the switch waits until the `core` cursor has reached the `raw` cursor.
4. Drain the buffer from `H` on. Stop the parallel workers. The state is now `live`.

Subscribing before fetching the last range is what closes the gap: a block is either in the range
or in the buffer, and a block in both is the same natural key, so the second write is a no-op.

**`live`.** Logs arrive pushed, with no polling. Per block the fetcher sends at most one multicall:
prices plus whatever the scheduler says is due. Every result is written once to `raw:{chain_id}`.
The fetcher reads the `settle_tag` block at a slow interval (one small request); for each block now
at or below it whose hash matches, it copies that block's raw rows from the stream to ClickHouse,
moves the raw cursor and publishes `settled` on `chain:{chain_id}`.

**Back to `catching_up`.** If the socket drops, or a head arrives that is not the next block, the
fetcher runs the switch again from step 1. If the gap is larger than one range it runs
`catching_up` first. Dragonfly losing `raw:{chain_id}` is the same case: the raw cursor in
ClickHouse is the truth and the gap is refetched.

**The processor follows.** It reads ClickHouse `raw.*` from its own cursor until that cursor meets
the raw cursor, then switches to reading `raw:{chain_id}`. On its first block from the stream it
loads the `*_latest` tables into Dragonfly, so the tip starts from full state. If it falls behind
the stream's retention it goes back to ClickHouse the same way.

**Built so far.** Both processes run on all four chains.

| Piece | Code | What it does |
|---|---|---|
| Logs, settled | `fetch.rs` | `catching_up` for logs with Spoke discovery; keeps walking as the settle tag advances |
| Calls, settled | `calls.rs` | walks from the `calls` cursor to the `raw` cursor over the stored logs, plans the views of each block (`model.rs`) and stores one multicall per block in `raw.calls` |
| Tip | `tip.rs` | `live`: follows the head and appends unsettled batches to `raw:{chain_id}`; owns the health factor tiers (`due:{chain_id}`) |
| Processor, settled | `process.rs` | `raw.*` to `core.*` and `meta.assets`, `meta.reserves`, `meta.tokens`, from the `core` cursor to the `calls` cursor |
| Aave v3 totals | `v3.rs` | hourly supplied and borrowed of every v3 market on 21 chains, into `raw.v3_calls`; see [Aave v3 totals](#aave-v3-totals) |
| Processor, tip | `live.rs` | `raw:{chain_id}` to the latest-state keys, the rankings and the `feed`, `state`, `risk`, `chain` streams |

Where the code is simpler than the design above, on purpose:

- `shortcut: three cursors, not two.` Calls were added after the logs were backfilled, so they
  have their own cursor (`calls`). "Complete up to here" for the processor is the `calls` cursor.
- `shortcut: the head is polled over HTTP every TIP_INTERVAL_MS (3s, or the block time if longer), not pushed over a WebSocket.`
  Logs of the blocks since the last poll come from one `eth_getLogs`; state is read once per poll,
  at the head, so an event is priced with a reading up to one interval old. Move to
  `eth_subscribe` and per-block reads when that matters.
- `shortcut: settled blocks are fetched again by the catch-up walk instead of being copied from raw:{chain_id}.`
  Each live block's requests are paid twice, and losing Dragonfly needs no recovery path. Copy
  from the stream when the request count matters.
- Health factor reads by tier are the one thing only the tip reads. The fetcher holds them until
  their block is settled, then writes them to `raw.calls` (`reason = tier`); the calls walk never
  passes a block whose reads are still held. So `user_account_latest` is at most one tier interval
  old once a chain is live, and a fetcher restart loses only the reads it was holding.
  Every account with a position is on the schedule, not only borrowers (a supplier with no debt is
  read hourly), and each read of an account also reads every reserve it holds, so
  `user_reserve_latest` is as fresh as the account.
- `shortcut: a reorg publishes reorg {from_block} and the replacing blocks; latest-state keys written by the replaced blocks stay until the next read overwrites them.`
- `shortcut: historical state is read at blocks with events and once an hour, with prices from getReservesPrices; if one source of a Spoke reverts, that Spoke has no price at that block.`
  Read per reserve (`getReservePrice`) if that is seen in `raw.calls` (`ok = 0`).
- Reparse is `infra/clickhouse/reparse.sh`: it resets the `core` cursor and the processor walks
  `raw` again from each chain's first block, replacing rows as it goes (nothing is dropped first, so
  the API keeps answering). It is always the whole range, not a chosen one. After a backfill or a
  reparse run `infra/clickhouse/rollup_backfill.sh`.
- `canonical` is the token's symbol in lower case, not a curated mapping: right for USDC on four
  chains, wrong the day two different tokens share a symbol. Replace with `registry/assets.toml`
  then.
- Not built: `lock:*`, `q:*`, `meta.price_sources` (the source is on each `core.prices` row
  instead), `labels`, token logos, `parser_runs`, receipts (gas, liquidator profit).

### RPC budget

What a node is asked for, so the cost can be counted in one place:

| Request | `catching_up` | `live` |
|---|---|---|
| Logs | one `eth_getLogs` per range, all addresses | none: pushed by the `logs` subscription |
| Headers | none if the node returns `blockTimestamp` on logs (Ethereum, Base and Arc on QuickNode do; Avalanche does not); else one batched `eth_getBlockByNumber` for blocks that have rows | none: pushed by `newHeads` |
| State | one multicall `eth_call` per block that had an event, plus the hourly heartbeat | at most one multicall `eth_call` per block |
| Settled tag | one read per round | one read per interval |

Rules that keep it there: no field getters, only the struct getters above; calls are chunked by
calldata size, not one per entity; nothing is called twice for the same block unless that block was
replaced by a reorg; the processor, the API and `reparse` never hold an RPC URL. `catching_up`
needs an archive node; `live` does not, so the two states may use different endpoints.

Provider limits met so far: public nodes cap a log filter at 10 addresses and refuse old blocks
(`FETCH_MAX_ADDRESSES` splits the filter, at one request per 10 addresses); QuickNode takes the
whole watch list and 10,000-block ranges, and counts every item of a batch against a limit of 300
requests per second.

### Scheduler (inside the fetcher)

The fetcher decodes just enough to plan calls (which entity an event touched, the health factor in
a result) using the same `crates/protocol` code as the parser. It does not write parsed rows.

| State | Read when |
|---|---|
| Hub asset, Hub asset × Spoke | the block had an event for it, plus an hourly heartbeat |
| Spoke reserve | the block had an event for it, plus hourly |
| Oracle prices | every block live (one multicall per chain); during backfill on blocks with events plus hourly |
| User position | the block had an event for that user; `live`: with every read of the account |
| User health factor | `live` only, by tier: HF < 1.1 every poll, < 1.5 every 5 min, the rest hourly, including accounts with no debt |
| Price source update time | hourly, with the heartbeat |

Tier reads are not replayed in `catching_up`: the tier depends on the previous reading, so history
would have to be walked one block at a time on an archive node. Historical health factor exists at
the blocks where the user had an event.

The schedule state (who is due when) lives in a Dragonfly sorted set, so a restart resumes it; the
switch to `live` builds it. Timers are tokio intervals keyed on block numbers, not wall-clock
cron.

Rollups are ClickHouse refreshable materialized views (below). With those two, nothing outside the
indexer and ClickHouse runs on a schedule.

### Reorgs

Only settled blocks reach ClickHouse, so ClickHouse is never rewritten in normal operation. The
unsettled tip lives in Dragonfly, where replacing a block is cheap:

1. A new head whose parent hash does not match, or a log pushed with `removed: true`: find the
   fork block.
2. Publish `reorg {from_block}` on `chain:{chain_id}` and on `raw:{chain_id}`. The processor and
   the clients drop unsettled items from that block.
3. Fetch the replaced blocks (logs and calls) and publish again. This is the one case where a
   block height is fetched twice.

A settled block whose hash later changes is an incident, not a case to handle: the fetcher stops
that chain and logs it. The fix is to delete rows from the fork block up in `raw` and `core` and
backfill that range. `raw.blocks` keeps hashes so this is detectable.

### Restart

Each process, per chain: read its cursor and resume from cursor + 1. A crash can land between a
batch and the cursor, so rows above the cursor may exist; they are written again, and every table
is a `ReplacingMergeTree` on the natural key, so that is harmless. Nothing is deleted. The fetcher
always restarts in `catching_up`, which is a short walk when it was `live` a moment ago.

## Aave v3 totals

Read so v4 can be compared with v3 (the Adoption page). Totals only: no positions, no events.

- **Markets.** `registry/v3.toml`: 24 markets on 21 chains, from Aave's address book. A chain is
  read when `RPC_HTTP_{chain_id}` is set; most of these chains have no v4 and no entry in
  `registry/chains.toml`.
- **A reading** is two multicalls at one block per market. First `getPriceOracle` and
  `getPoolDataProvider` on the market's PoolAddressesProvider and `getReservesList` on its Pool,
  so the addresses are the ones in force at that block. Then, per reserve,
  `getATokenTotalSupply`, `getTotalDebt`, `getAssetPrice` and the token's `decimals`, plus the
  oracle's `BASE_CURRENCY_UNIT`.
- **When.** One reading per hour for the last 7 days and one per day (00:00 UTC) back to
  2026-03-24, the first v4 day. The block is found by time: any block in the hour's first 20
  minutes. An hour is read once it is 30 minutes old.
  `shortcut: the settle tag is not used for v3; a reorg deeper than 30 minutes would leave a stale reading.`
- **Stored** as returned in `raw.v3_calls` (`chain_id, market, block_number, block_time, target,
  selector, args, ok, result`). A row with `market = ''` marks the hour as done for the chain; the
  task reads every due hour that has no marker, newest first, so a restart or a new chain
  backfills by itself.
- **Decoded** in SQL by the view `core.v3_reserve_state` (one row per reserve per reading:
  `supplied, borrowed, price, supplied_usd, borrowed_usd`). Every answer it uses is one uint256.
- **Gaps.** When a node no longer has a block's state the market is skipped for that hour and the
  hour is still marked done. The provider keeps only recent state on Monad and MegaETH, so their
  v3 history starts later than the others. Harmony and Fantom (frozen, nothing supplied) and
  Aptos (not EVM) are not read.
- **Not the same as DefiLlama's TVL:** every reserve and every supplier is counted; DefiLlama
  leaves out some tokens and Ethena's backing wallets, and lists the Ink market as a separate
  protocol. Borrowed matches it within 1% per chain (2026-11-18).

## Prices

Each Spoke has one `AaveOracle`. Each reserve has a source that answers `latestAnswer()` and
`decimals()` (Chainlink's interface, whatever sits behind it).

- **Price of record:** `AaveOracle.getReservesPrices` at the block. It is the price the protocol
  liquidates with, so it is the price we show.
- **Source detail:** the Spoke's `UpdateReservePriceSource` event gives the source address, written
  to `core.prices.source`. Once an hour (the heartbeat) each source is asked `latestRoundData()`;
  its `updatedAt` goes to `core.prices.source_updated_at`, and a new update time writes a price row
  even when the price did not move. That is what "stale price" means. Checked 2026-11-18: 25 of
  the 59 sources answer it; the rest are adapters with only `latestAnswer()`, and have no update
  time. Read from 2026-11-18 on: not backfilled.
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
  reason LowCardinality(String),      -- why it was read: event, heartbeat, tier, meta
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

Rollups: `events_hourly`, `events_daily`, `hub_asset_hourly` (last state per hour, for supply,
debt, usage and rate charts), `hub_spoke_hourly`, `reserve_hourly`, `liquidations_daily`. More are
added when a panel needs one, not before.

Two plain views compute at read time (`0009_derived.sql`): `core.liquidations`, one row per
`LiquidationCall` with debt, collateral seized, bonus and protocol fee in USD; and
`core.hub_asset_apy`, the supply APY measured from the share price over 24 hours and 7 days.

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
| `cursor` | `chain_id, stream` | last complete settled block and hash. Three streams: `raw` (fetcher, logs), `calls` (fetcher, view calls) and `core` (processor) |
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
| `raw:{chain_id}` | one batch of unsettled blocks as fetched: the head's header, the logs since the last batch, the call results at the head. Written by the fetcher, read by the processor (its place is `tipcursor:{chain_id}`), not relayed to browsers | 1 hour |

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
| `lock:fetcher`, `lock:processor` | string, TTL | one of each at a time |

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
| Configurator and position manager activity | add their addresses to the watch list (not known yet; the `AccessManager` is watched) | logs of those contracts |
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
4. Live price reads: every block, or only when a feed logs `AnswerUpdated`. The second removes most
   live `eth_call`s but needs every price source to be a feed that logs; check against
   [protocol.md](protocol.md) before choosing.
5. Whether the chosen provider's `logs` subscription takes the whole watch list in one filter and
   sends `removed` logs on every chain. If not, `live` falls back to one `eth_getLogs` per head.
