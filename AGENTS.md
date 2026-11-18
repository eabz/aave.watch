# aave.watch

Monitoring and real-time app for Aave v4. Main reader: a risk analyst.

## Read before you change anything

| Doc | Read it when you touch |
|---|---|
| [docs/protocol.md](docs/protocol.md) | contract addresses, ABIs, events, view calls |
| [docs/architecture.md](docs/architecture.md) | repo layout, indexer, API, ClickHouse, Dragonfly, Worker |
| [docs/dashboard-data.md](docs/dashboard-data.md) | any number shown in the UI, any API endpoint, any query |
| [docs/branding.md](docs/branding.md) | any UI, colour, font, copy or chart |
| [docs/visual-identity.html](docs/visual-identity.html) | any component: copy its markup, tokens and states from here |
| [PRODUCT.md](PRODUCT.md) | users, purpose, brand commitments, what must not be invented |
| [README.md](README.md) | project entry point; keep its doc list in sync with this table |

The docs are the spec. If code and a doc disagree, fix one of them in the same change. A new doc
gets a row here and a line in the README.

## Stack

- `apps/web`: Vite + React. `apps/worker`: Cloudflare Worker (API proxy, SSE relay, static assets).
- `crates/indexer`, `crates/api`, `crates/protocol`: Rust.
- ClickHouse for storage, Dragonfly for latest state, streams and cache.
- pnpm workspaces for `apps/*`, a Cargo workspace for `crates/*`.

## Rules

- State comes from contract views at a block; activity comes from events. Do not rebuild balances
  from events.
- Every state number carries its block.
- UI uses the tokens in `docs/branding.md` only. No Aave logos or proprietary Aave fonts.
- `docs/branding.md` and `docs/visual-identity.html` change together.
- Preview the identity page with the `docs` server in `.claude/launch.json`.
- Status: scaffold only. The crates and apps build and do nothing yet.
