# Architecture

## Flow

```
Chain RPCs (WebSocket heads + eth_getLogs + multicall), one set per chain
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
  protocol/     deployed ABIs (abi/*.json), Dragonfly key names, shared row and message types
  indexer/      binary: chain -> ClickHouse + Dragonfly
  api/          binary: HTTP + SSE over ClickHouse and Dragonfly
registry/       chains and Hubs (chains.toml), Spokes with kind and name (spokes.toml)
infra/
  clickhouse/   numbered .sql migrations and migrate.sh
  dragonfly/    dragonfly.conf
  systemd/      units for indexer and api (no containers; see Server)
docs/
```

Two workspaces at the root: `pnpm-workspace.yaml` for `apps/*`, `Cargo.toml` for `crates/*`. No
Turborepo, no Nx.

Serving the web build from the same Worker keeps one origin: no CORS, one deploy.

Deploy: `pnpm --filter worker run deploy` builds `apps/web`, then uploads the Worker and the assets.
Live at https://aave.watch (and `www`; the workers.dev URL is off). Cloudflare Workers Builds runs the same command on
every push to `main` (root directory `/`, deploy command `pnpm --filter worker run deploy`).

## Indexer, ClickHouse, Dragonfly

The stages, the ClickHouse schema and the Dragonfly keys are specified in
[data-pipeline.md](data-pipeline.md). In short:

- **Indexer.** One Rust process for every chain (Ethereum, Arc, Avalanche, Base), one task group
  per chain. `alloy` for RPC and ABI decoding, `clickhouse` and `redis` crates. It owns every
  schedule; there is no external cron. Three modes share one code path: `live`, `backfill`,
  `reparse`.
- **ClickHouse** holds settled history in three databases: `raw` (what nodes returned), `core`
  (parsed and enriched facts, latest-state tables, rollups) and `meta` (registry, tokens, cursor).
  `core` can be rebuilt from `raw` without a node.
- **Dragonfly** holds the unsettled tip, the latest state per entity, sorted sets for the live risk
  view, and capped streams (`feed`, `state`, `risk`, `chain`, one set per chain) that the API
  relays to browsers.
- **Reorgs** never touch ClickHouse in normal operation: a block is written there only once it is
  at or below its chain's `settle_tag` (`finalized`, or `safe` on Base).

## API (Rust)

`axum`. Reads only.

- `GET /v1/...` JSON endpoints, one per dashboard panel (see [dashboard-data.md](dashboard-data.md)).
  Each response sets `Cache-Control` so the Worker can cache it at the edge.
- `GET /v1/stream?chain=1&topics=feed,state,risk&hub=...&spoke=...&min_usd=...` Server-Sent Events. One reader
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
- Auth, API keys, user accounts.

## Open decisions

1. RPC provider. Backfill and per-block multicalls need an archive node with a generous rate limit.
2. Whether the server above is big enough: 436 GiB of disk against backfill volume, measured after
   the first backfill.
3. Health factor polling cost. It scales with the number of borrowers; measure after backfill and
   tune the tiers.
