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
                              /og/*.png  share images, drawn on request
                              /*  static web assets, page head per path
                                         ▲
                                  web (Vite + React)
```

Nothing on the server listens on a public port. The tunnel is the only way in, and the Worker is the
only client that can use it (a Workers VPC binding; the tunnel has no public hostname).

## Repo layout

```
apps/
  web/          Vite + React dashboard
  worker/       Cloudflare API Worker (also serves web's build as static assets)
crates/
  protocol/     deployed ABIs (abi/*.json), Dragonfly key names, shared row and message types
  indexer/      binary, two processes: `fetch` (node -> raw) and `process` (raw -> core, tip)
  api/          binary: HTTP + SSE over ClickHouse and Dragonfly
registry/       chains and seed Hubs (chains.toml), seed Spokes with kind and name (spokes.toml), Aave v3 markets (v3.toml)
infra/
  clickhouse/   numbered .sql migrations and migrate.sh
  dragonfly/    dragonfly.conf
  systemd/      units for indexer-fetch, indexer-process and api (no containers; see Server)
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

- **Indexer.** One Rust binary, two processes, each covering every chain (Ethereum, Arc,
  Avalanche, Base) with one task group per chain. `indexer fetch` is the only process that talks to
  a node: it stores what the node returned and asks for nothing twice. It catches up over HTTP with
  parallel ranges, then switches by itself to one WebSocket per chain. `indexer process` parses and
  enriches what the fetcher stored and never holds an RPC URL. The fetcher speaks plain JSON-RPC (`reqwest`) and stores hex as returned; `alloy` for ABI decoding,
  `clickhouse` and `redis` crates. The fetcher owns every schedule; there is no external cron.
  Hubs and Spokes are not a fixed list: `registry/*.toml` is a seed, `meta.hubs` and `meta.spokes`
  are the list, and the fetcher adds to them what it finds on chain
  ([Hub discovery](data-pipeline.md#hub-discovery)). For that the fetcher makes its one request
  that is not to a node: Aave's app API, every ten minutes, for names and for whether Aave lists a
  new Hub. Nothing stops when it fails.
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

- `/v1/*` (not stream): forward to the API, cache GETs at the edge per the API's `Cache-Control`.
- `/v1/stream`: pass the SSE response body through. One connection to the API per client.
- `/og/<page path>.png`: the page's share image, drawn on request (below).
- Pages (any other path that is not a file): `index.html` from `apps/web`, with the head rewritten
  for the path (below).
- `/assets/*`, `/icons/*`, `/fonts/*`, `/favicon.svg`, `/robots.txt`: static files from `apps/web`, served without
  running the Worker (`run_worker_first` in `apps/worker/wrangler.jsonc`). A new top-level folder
  of static files in `apps/web/public` is added to that list.

**Share images and link previews.** `apps/worker/src/share.ts` is the one place that knows what a
page is called: it maps a path (`/`, `/rates`, `/close-calls`, `/token/<symbol>`,
`/pool/<chain id>/<address>`, `/market/<chain id>/<address>`, `/wallet/<address>`; case does not
matter) to a title, a description and the content of its share image, and holds the image's SVG
template. It is pure, so it is tested with `pnpm --filter worker test:share` (plain Node, no
Cloudflare account) and previewed with `node scripts/og-samples.mts`. What the images show is in
[branding.md](branding.md#share-images-and-favicon).

- **Image.** The Worker reads the figures through the `API` binding (3 s limit; `/v1/summary`,
  `/v1/hub-assets`, `/v1/hubs`, `/v1/spokes`, `/v1/reserves`, and `/v1/account` once per chain for
  a wallet) and the logos through the `ASSETS` binding, fills the template and renders it to PNG
  with `@resvg/resvg-wasm`; the three TTFs in `apps/worker/src/fonts/` are bundled with the code.
  The answer carries `Cache-Control: public, max-age=3600` and is put in the Cache API under its
  path, so a path is drawn at most once an hour per data centre. If a read fails the image keeps
  its fixed sentence and is cached for 60 seconds. An unknown token, pool or market answers 404.
- **Page head.** `HTMLRewriter` sets `<title>`, `meta[name=description]`, `og:title`,
  `og:description`, `og:url`, `og:image`, `og:image:alt` and `link[rel=canonical]` (added if the
  page has none). It only changes tags that `apps/web/index.html` already has. A pool or market
  page costs one API read for its name (`/v1/hubs` or `/v1/spokes`, cached 5 minutes at the edge);
  if that fails the title uses the short address. A path the list above does not know gets the home
  text and its own address.
- **Size and plan.** The upload is 2.6 MB, 1.04 MB gzipped (the renderer is 2.4 MB of it), inside
  the free plan's 3 MB. Drawing one image takes 30 to 120 ms of CPU, which is over the free plan's
  10 ms per request: the Worker needs the paid plan. Every page view now runs the Worker too.

**How the Worker reaches the box.** Through a Cloudflare Tunnel and a Workers VPC service, not a
public hostname:

| Piece | Name | Id |
|---|---|---|
| Tunnel (outbound from the box, `cloudflared` service) | `aave-watch-box` | `82b1dc49-4a33-47ea-8c4f-24aa8a1bd8a8` |
| VPC service (HTTP to `127.0.0.1:8080` at the tunnel's end) | `aave-watch-api` | `01a1ef52-9070-7be1-880c-da6f8754fb56` |
| Worker binding | `API` in `apps/worker/wrangler.jsonc` | |

The Worker calls `env.API.fetch("http://127.0.0.1:8080/v1/...")`. No DNS record, no Access policy and
no secret are involved, and the box keeps no public port for it. The tunnel holds four QUIC
connections to Cloudflare's Frankfurt data centres, about 6 ms from the box. Both resources live in
the Cloudflare account that owns the Worker; manage them with `wrangler tunnel` and
`wrangler vpc service`.

The Worker cannot subscribe to Dragonfly itself. Dragonfly is private and speaks TCP; the Rust API
does the subscribing and the Worker relays its stream.

`shortcut: one API connection per SSE client; move fan-out into a Durable Object (one upstream connection, many clients) when concurrent viewers load the server.`

`shortcut: no rate limit on the Worker, and /og/wallet/<any address>.png costs four API reads and a render; add a per-IP rate-limit binding before the site is announced.`

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

Every Rust service is built from that clone and installed by one script, `infra/deploy.sh`: it
builds the workspace, installs the units and the binaries, applies the ClickHouse migrations, loads
the registry and restarts the services. Run it after pulling `main`. `pnpm` 11 is installed globally
with `npm` for building `apps/*` on the box.

The API runs as the systemd unit `aave-watch-api` (binary `/usr/local/bin/aave-watch-api`), on
`127.0.0.1:8080`, under a systemd dynamic user. Installed 2026-11-18. See [api.md](api.md).

The fetcher runs as the systemd unit `aave-watch-fetch` (binary `/usr/local/bin/aave-watch-indexer fetch`,
unit file in `infra/systemd/`, `DynamicUser`). Its RPC URLs, which carry the provider key, are in
`/etc/aave-watch/fetch.env` (root only; template in `infra/systemd/fetch.env.example`): the four
v4 chains and 17 more for the Aave v3 totals, the same provider on each except Metis, Monad and
MegaETH (public nodes; the provider keeps no old state for the last two). The
processor is the same binary as the unit `aave-watch-process` (`aave-watch-indexer process`); it has
no RPC URL.

`aave-watch-indexer daily` is a one-off command, not a unit: it backfills one account reading a day
for the wallet history charts and needs the fetcher's environment. How to run it:
[data-pipeline.md](data-pipeline.md#wallet-history).

`aave-watch-indexer backfill-hubs` is the other one-off command: it syncs the Hubs created before
the fetcher could find them, in its own process (a transient unit, `aave-watch-backfill-hubs`), with
the fetcher's environment. Run on 2026-11-19. How to run it:
[data-pipeline.md](data-pipeline.md#hub-discovery).

`cloudflared` 2026.11.1 is installed from Cloudflare's apt repo (`pkg.cloudflare.com`) and runs as the
systemd unit `cloudflared`, connected as the tunnel `aave-watch-box`. It makes outbound connections
only. Its token is in the unit file; rotate it from the Cloudflare dashboard if the box is rebuilt.

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
