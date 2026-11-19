# API

The read-only HTTP API served by `crates/api`, reached by browsers through the Worker at `/v1/*`.
It reads ClickHouse `core.*` and `meta.*` and the Dragonfly tip; it never talks to a node. Storage
is specified in [data-pipeline.md](data-pipeline.md).

## Conventions

- `GET` only. JSON. Every list answers `{ "data": [ ... ] }`; errors answer
  `{ "error": "what failed" }` with a 4xx or 5xx status.
- Field names are the ClickHouse column names of the table behind the endpoint.
- Addresses are lowercase `0x` hex, in parameters and in answers.
- Token amounts, shares and other 64-bit-or-wider integers are **strings** (they do not fit a JS
  number). USD values, prices, rates and health factors are numbers. `health_factor: null` means
  no debt.
- Times are `YYYY-MM-DD hh:mm:ss`, UTC.
- Every state row carries `block_number` and `block_time`: the UI shows them as the reading stamp.
- `chain` is a chain id (`1`, `8453`, `43114`, `5042`). Where it is optional, leaving it out means
  all chains.
- `Cache-Control: public, max-age=N` on every answer; the Worker caches on it.

## Endpoints

| Path | Parameters | Answers (one item per) | Source | max-age |
|---|---|---|---|---|
| `/v1/health` | | `{ clickhouse, dragonfly, heads: [{chain_id, number, settled, time}] }` | both | 0 |
| `/v1/chains` | | chain: `chain_id, name, block_time_ms, settle_tag, explorer_url` | `meta.chains` | 300 |
| `/v1/hubs` | `chain?` | Hub: `chain_id, address, name, first_block` | `meta.hubs` | 300 |
| `/v1/spokes` | `chain?`, `kind?` | Spoke: `chain_id, address, name, kind, oracle, first_block` | `meta.spokes` | 300 |
| `/v1/summary` | `chain?` | chain: `chain_id, supplied_usd, borrowed_usd, liquidity_usd, positions_below_1_1, debt_below_1_1, liquidatable, block_number, block_time` | `core.*_latest` | 15 |
| `/v1/hub-assets` | `chain?`, `hub?` | Hub asset: the `core.hub_asset_latest` row plus `hub_name, token, symbol, decimals` and `supply_apy_1d, supply_apy_7d` (measured from the share price, a fraction; `null` when there is no reading that old) | `core.hub_asset_latest`, `core.hub_asset_apy` | 15 |
| `/v1/hub-spokes` | `chain?`, `hub?`, `asset_id?`, `spoke?` | Spoke line on a Hub asset: the `core.hub_spoke_latest` row plus `spoke_name, spoke_kind, symbol, decimals` | `core.hub_spoke_latest` | 15 |
| `/v1/reserves` | `chain?`, `spoke?` | reserve: the `core.reserve_latest` row plus `symbol, decimals` | `core.reserve_latest` | 15 |
| `/v1/positions` | `chain?`, `spoke?`, `max_hf?` (default 1.5), `limit?` (default 100, max 1000) | account: the `core.user_account_latest` row plus `spoke_name` (from the tip: `chain_id, spoke, spoke_name, user, health_factor, collateral_usd, debt_usd, risk_premium, block_number, block_time` only), sorted by health factor ascending. Accounts with no debt are left out | Dragonfly tip, else `core.user_account_latest` | 5 |
| `/v1/account` | `chain`, `user` | one per Spoke: the account row and `reserves: [core.user_reserve_latest rows]` | `core.*_latest` | 15 |
| `/v1/holders` | `symbol`, `chain?`, `by?` (`supplied`, default, or `debt`), `limit?` (default 20, max 200) | wallet: `user, supplied_usd, debt_usd, markets, chains, block_number, block_time` for one token, every market and chain added up, biggest first. The symbol matches without regard to case (USDT and USDt are one) | `core.user_reserve_latest` | 60 |
| `/v1/events` | `chain?`, `spoke?`, `hub?`, `user?`, `name?`, `symbol?`, `before?` (`block_number`), `limit?` (default 100, max 1000) | event: the `core.events` row plus `symbol, spoke_name`, newest first. `args` is a JSON object | `core.events` | 15 |
| `/v1/liquidations` | `chain?`, `spoke?`, `user?`, `liquidator?`, `before?` (`block_number`), `limit?` (default 100, max 1000) | liquidation: the `core.liquidations` row (`debt_usd, collateral_usd, bonus_usd, fee_usd`, both reserves and tokens, `tx_hash`) plus `debt_symbol, collateral_symbol, spoke_name`, newest first | `core.liquidations` | 15 |
| `/v1/history/hub-asset` | `chain`, `hub`, `asset_id`, `hours?` (default 720) | hour: the `core.hub_asset_hourly` row, oldest first | `core.hub_asset_hourly` | 300 |
| `/v1/history/totals` | `chain?`, `hub?` (one Hub only), `step?` (`hour`, default, or `day`), `hours?` (default 720) | chain and hour: `chain_id, hour, supplied_usd, borrowed_usd, liquidity_usd, block_number, block_time`, oldest first. An asset with no reading in an hour keeps its last one. `step=day` keeps the 00:00 UTC reading of each day and the newest hour. `block_time` is the hour, not the block's own time | `core.hub_asset_hourly` | 300 |
| `/v1/history/v3-totals` | same as `/v1/history/totals` | Aave v3, every market of a chain added up: the same fields plus `chain` (the chain's name; v3 runs on chains `/v1/chains` does not list). `liquidity_usd` is supplied minus borrowed | `core.v3_reserve_state` | 300 |
| `/v1/history/account` | `chain`, `user`, `spoke?`, `step?` (`hour`, default, or `day`), `hours?` (default 720) | Spoke and hour: `chain_id, spoke, spoke_name, hour, health_factor, collateral_usd, debt_usd, risk_premium, block_number, block_time`, sorted by Spoke, then oldest first. Without `spoke`, every Spoke the wallet is in. From the wallet's first reading to its newest, an hour with no reading keeps the last one, with that reading's `block_number` and `block_time`. `step=day` keeps the 00:00 UTC hour of each day and the newest hour. Before 2026-11-18 there is one reading a day plus the wallet's own events | `core.user_account_hourly` | 300 |

`/v1/positions` answers with `"source": "tip"` or `"source": "settled"` next to `data`, so the UI
can say how fresh the list is.

## Stream

`GET /v1/stream` is Server-Sent Events. It relays the Dragonfly streams of
[data-pipeline.md](data-pipeline.md#streams-pubsub-with-replay).

| Parameter | Meaning |
|---|---|
| `chain` | chain ids, comma separated. Default: all |
| `topics` | any of `feed`, `state`, `risk`, `chain`, comma separated. Default: `feed,chain` |
| `spoke`, `hub`, `user` | keep only items whose `data` has that value |
| `min_usd` | keep only `feed` items with `data.amount_usd` at or above it |

Each SSE message:

```
id: feed:1/1795012264330-0
event: feed
data: {"v":1,"chain_id":1,"block":21480112,"block_hash":"0x…","log_index":14,"settled":false,"type":"event","name":"Borrow","data":{…}}
```

- `event` is the topic. `data` is the envelope the indexer wrote, unchanged.
- The envelope's `type` says what its `data` is, with the fields of the endpoint that serves the
  same row: on `feed`, `event` (a `/v1/events` row); on `state`, `hub_asset`, `hub_spoke`,
  `reserve` or `price`; on `risk`, `band` (a `/v1/positions` row plus `from_band` and `to_band`:
  `liquidatable`, `below_1_1`, `below_1_5`, `healthy`) or `event` (a `LiquidationCall`); on
  `chain`, `head`, `settled` or `reorg`.
- `id` is the stream name and the entry id. The browser sends the last one back as `Last-Event-ID`
  when it reconnects, and the API replays what was missed (up to one hour).
- Replay across several topics is by time: after a reconnect an item may arrive twice. Items are
  identified by `(chain_id, block, log_index, type)`; the client drops repeats.
- A `chain` message with `"type":"reorg"` and `from_block` means: drop unsettled items from that
  block on.
- A comment line is sent every 15 seconds to keep the connection open.
- `event: lagged` means this client fell behind the server's buffer and missed items: reload the
  list it shows.
- If Dragonfly is unreachable the endpoint answers `503`; the rest of the API keeps working from
  ClickHouse.

## Configuration

Environment variables of `crates/api`:

| Variable | Default | Meaning |
|---|---|---|
| `API_LISTEN` | `127.0.0.1:8080` | address to listen on |
| `CLICKHOUSE_URL` | `http://127.0.0.1:8123` | ClickHouse HTTP interface |
| `DRAGONFLY_URL` | `redis://127.0.0.1:6379` | Dragonfly |
| `CHAINS` | `1,8453,43114,5042` | chains whose streams are relayed |

## Running it

- `aave-watch-api` serves the API.
- `aave-watch-api load-registry` copies `registry/*.toml` into `meta.chains`, `meta.hubs` and
  `meta.spokes` and exits. Run it after a registry change; it is safe to repeat. Assets, reserves
  and tokens are written by the indexer, not by this command.
- On the server it runs as the systemd unit `aave-watch-api`
  ([infra/systemd/aave-watch-api.service](../infra/systemd/aave-watch-api.service)), bound to
  `127.0.0.1:8080`, built from the repo at `/root/aave.watch` by `infra/deploy.sh`.
- Locally: apply the migrations to a ClickHouse, load
  [infra/clickhouse/dev/seed.sql](../infra/clickhouse/dev/seed.sql) (sample data, never on the
  server), run the API, then `pnpm --filter web dev` (it proxies `/v1` to `127.0.0.1:8080`).

When Dragonfly does not answer within 750ms the API treats it as down: `/v1/positions` answers from
ClickHouse with `"source": "settled"`, `/v1/stream` answers `503`.

The Worker has no variables or secrets for the API. It reaches it through the `API` binding in
`apps/worker/wrangler.jsonc`: a Workers VPC service over the Cloudflare Tunnel (see
[architecture.md](architecture.md#api-worker-cloudflare)).
