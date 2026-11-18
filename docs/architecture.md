# Architecture

## Flow

```
Ethereum RPC (WebSocket heads + eth_getLogs + multicall)
        │
        ▼
  indexer (Rust) ───── insert ─────▶ ClickHouse
        │                                ▲
        │ XADD streams, SET latest       │ SQL
        ▼                                │
    Dragonfly ◀──── read / cache ──── api (Rust)          ── one server ──
                                         ▲
                                  cloudflared tunnel
                                         ▲
                              API Worker (Cloudflare)      ── edge ──
                              /v1/*  proxy + edge cache
                              /v1/stream  SSE relay
                              /*  static web assets
                                         ▲
                                  web (Vite + React)
```

Nothing on the server listens on a public port. The tunnel is the only way in, and the Worker is the
only client allowed through it (Cloudflare Access service token on the tunnel hostname).

## Repo layout

```
apps/
  web/          Vite + React dashboard
  worker/       Cloudflare API Worker (also serves web's build as static assets)
crates/
  protocol/     ABIs (alloy sol!), addresses, shared row and message types
  indexer/      binary: chain -> ClickHouse + Dragonfly
  api/          binary: HTTP + SSE over ClickHouse and Dragonfly
infra/
  clickhouse/   numbered .sql migrations
  systemd/      units for indexer and api (no containers; see Server)
docs/
```

Two workspaces at the root: `pnpm-workspace.yaml` for `apps/*`, `Cargo.toml` for `crates/*`. No
Turborepo, no Nx.

Serving the web build from the same Worker keeps one origin: no CORS, one deploy.

Deploy: `pnpm --filter worker run deploy` builds `apps/web`, then uploads the Worker and the assets.
Live at https://aave-watch.kindynos.workers.dev. Cloudflare Workers Builds runs the same command on
every push to `main` (root directory `/`, deploy command `pnpm --filter worker run deploy`).

## Indexer

One process per chain. Rust with `alloy` (RPC, ABI decoding), `clickhouse` and `redis` crates.

**Registry.** Starts from the seed list in [protocol.md](protocol.md), then follows `Hub.AddAsset`,
`Hub.AddSpoke` and `Spoke.AddReserve` to learn every asset, Spoke and reserve. A Spoke it has not
seen before is added to the log filter from that block on.

**Events.** Backfill with `eth_getLogs` over block ranges from the deployment block. Live: on each
new head, one `eth_getLogs` for all Hub and Spoke addresses, decode, batch insert, publish.

**State.** Read from contract views with multicall at a fixed block, never rebuilt from events.
What triggers a read:

| State | Read when |
|---|---|
| Hub asset, Hub asset × Spoke | the block had an event for that asset, plus every hour |
| Spoke reserve | the block had an event for that reserve, plus every hour |
| Oracle prices | every block (one multicall per Spoke oracle), stored only on change |
| User position | the block had an event for that user |
| User health factor | on a schedule by risk: HF < 1.1 every block, < 1.5 every 5 min, rest hourly |

The hourly read exists because interest accrues between events.

**Reorgs.** The indexer follows `latest` and keeps the hash of each block it processed. If a new
head's parent hash does not match, it deletes rows from the fork block up (ClickHouse lightweight
`DELETE`), re-indexes, and publishes a `reorg` message so clients drop those events. Rare and
shallow on Ethereum, so the cost of a delete is acceptable.

**Cursor.** Last processed block and hash are stored in ClickHouse, so a restart resumes and
re-inserts are safe (tables dedupe on their key).

## ClickHouse

One server instance. Tables, all keyed by `chain_id` first:

| Table | Engine | One row is |
|---|---|---|
| `events` | ReplacingMergeTree, order `(chain_id, block_number, log_index)` | one decoded log: common columns (`contract`, `kind`, `name`, `hub`, `spoke`, `asset_id`, `reserve_id`, `user`, `caller`, `amount`, `shares`, `amount_usd`) plus the full args as JSON |
| `hub_asset_state` | MergeTree, order `(chain_id, hub, asset_id, block_number)` | one read of a Hub asset |
| `hub_spoke_state` | MergeTree, order `(chain_id, hub, asset_id, spoke, block_number)` | one read of a Spoke's line on a Hub asset |
| `reserve_state` | MergeTree, order `(chain_id, spoke, reserve_id, block_number)` | one read of a Spoke reserve |
| `prices` | MergeTree, order `(chain_id, spoke, reserve_id, block_number)` | one price change |
| `user_reserve_state` | MergeTree, order `(chain_id, spoke, user, reserve_id, block_number)` | one read of a user's position in one reserve |
| `user_account_state` | MergeTree, order `(chain_id, spoke, user, block_number)` | one read of `getUserAccountData` |
| `registry_*` | ReplacingMergeTree | hubs, spokes, assets, reserves, tokens |
| `cursor` | ReplacingMergeTree | last indexed block per chain |

One wide `events` table instead of one table per event: 40 event types, same queries (filter by
type, hub, spoke, user, time). Charts read from materialized views (hourly and daily rollups), added
when a chart needs one.

Amounts are stored raw (`UInt256`) and as USD (`Float64`) priced at the event's block.

## Dragonfly

Two uses.

**Latest state** (plain keys, written by the indexer): `hub:{addr}:asset:{id}`,
`spoke:{addr}:reserve:{id}`, `price:{spoke}:{reserveId}`, `head`. The API serves "current" numbers
from here without touching ClickHouse.

**Real-time topics.** Capped streams (`XADD ... MAXLEN ~`), not plain pub/sub:

| Stream | Messages |
|---|---|
| `feed` | every decoded event, with USD amount |
| `state` | changed Hub asset, reserve and price rows |
| `chain` | new head, reorg |

Why streams: plain pub/sub drops whatever is published while a client is disconnected. A stream has
ids, so a client that reconnects sends its last id and gets what it missed. Same server, same
commands family, no extra moving part.

**Query cache.** The API caches ClickHouse results for heavy endpoints here with a short TTL.

## API (Rust)

`axum`. Reads only.

- `GET /v1/...` JSON endpoints, one per dashboard panel (see [dashboard-data.md](dashboard-data.md)).
  Each response sets `Cache-Control` so the Worker can cache it at the edge.
- `GET /v1/stream?topics=feed,state&hub=...&spoke=...&min_usd=...` Server-Sent Events. One reader
  task per stream fans messages out in-process to connected clients; filters are applied per
  client. `Last-Event-ID` resumes from the stream.

SSE over WebSocket: the data only flows server to client, the browser reconnects and resumes by
itself, and it is plain HTTP through the tunnel and the Worker.

## API Worker (Cloudflare)

- `/v1/*` (not stream): forward to the tunnel hostname, cache GETs at the edge per origin
  `Cache-Control`, rate limit per IP.
- `/v1/stream`: pass the SSE response body through. One origin connection per client.
- Everything else: static assets from `apps/web`.

The Worker cannot subscribe to Dragonfly itself. Dragonfly is private and speaks TCP; the Rust API
does the subscribing and the Worker relays its stream.

`shortcut: one origin connection per SSE client; move fan-out into a Durable Object (one upstream connection, many clients) when concurrent viewers load the server.`

## Server

One dedicated box, SSH alias `aave-box` (hostname `aave.watch`, user `root`, key `~/.ssh/aave`). The
address lives in each operator's `~/.ssh/config`, not in this repo. Inventory taken 2026-11-18 on a
fresh install.

| | |
|---|---|
| CPU | Intel Core i7-6700, 4 cores / 8 threads, 3.4 GHz, x86_64 |
| RAM | 62 GiB, plus 32 GiB swap |
| Disk | 2 × Samsung 512 GB NVMe (MZVLB512HAJQ) in software RAID1 (mdadm): `/` ext4 436 GiB usable, `/boot` ext3 1 GiB, swap 32 GiB |
| Network | one NIC, public IPv4 and IPv6 |
| OS | Ubuntu 26.04.1 LTS, kernel 7.0.0-43 |

Installed: the base Ubuntu server image only (670 packages). Useful ones already there: `git`, `gh`,
`build-essential` (gcc 15.2, make), `python3` 3.14, `curl`, `tmux`, `htop`, `ufw`, `nftables`,
`mdadm`, `chrony`, `unattended-upgrades`.

No containers. Every service is a native package run by systemd, enabled at boot:

| Service | Version | Installed from | Unit, user | Listens on | Data | Config |
|---|---|---|---|---|---|---|
| ClickHouse | 26.10.6.18 | apt repo `packages.clickhouse.com/deb stable` | `clickhouse-server`, `clickhouse` | `127.0.0.1` and `::1`: 8123 HTTP, 9000 native (also 9004, 9005, 9009) | `/var/lib/clickhouse` | `/etc/clickhouse-server/` |
| Dragonfly | 2.1 | `dragonfly_amd64.deb` from the GitHub release | `dragonfly`, `dfly` | `127.0.0.1:6379` | `/var/lib/dragonfly` | `/etc/dragonfly/dragonfly.conf` |

ClickHouse upgrades with `apt`. Dragonfly has no apt repo: upgrade by installing the newer `.deb`.
The only change from package defaults is `--bind=127.0.0.1` in `dragonfly.conf` (it binds every
interface otherwise).

`shortcut: neither service has a password (ClickHouse user default, Dragonfly no requirepass); both are reachable only from localhost. Set passwords before any process we do not own runs on the box.`

`shortcut: memory limits are package defaults, and each service assumes it owns the RAM. Cap Dragonfly (--maxmemory) and ClickHouse (max_server_memory_usage) once backfill shows real usage.`

Rust 1.100 is installed with `rustup` for `root` (`/root/.cargo/bin`); upgrade with `rustup update`. The repo is cloned at `/root/aave.watch`.

Not installed yet: `cloudflared`.

Hardening, applied 2026-11-18:

- SSH is key-only: `/etc/ssh/sshd_config.d/00-hardening.conf` sets `PasswordAuthentication no` and
  `KbdInteractiveAuthentication no`; `PermitRootLogin prohibit-password`. Two keys are authorized,
  `0xeabz` and `aave`.
- `ufw` is on and enabled at boot: deny inbound, allow outbound, allow `22/tcp` (v4 and v6).

Port 22 is the one public port, the single exception to "nothing listens on a public port". Every
service binds to localhost and is reached through the tunnel; do not open another port in `ufw`.

Still to do:

- No non-root user exists. Create one per daemon when the daemons are installed.
- Consider moving SSH behind the tunnel and closing port 22.

## Not in this version

- Alerts (Telegram, Discord, email). Later: a third Rust binary that reads the same streams.
- Avalanche. The code is keyed by chain id; adding a chain is config plus a second indexer process.
- Auth, API keys, user accounts.

## Open decisions

1. RPC provider. Backfill and per-block multicalls need an archive node with a generous rate limit.
2. Whether the server above is big enough: 436 GiB of disk against backfill volume, measured after
   the first backfill.
3. Health factor polling cost. It scales with the number of borrowers; measure after backfill and
   tune the tiers.
