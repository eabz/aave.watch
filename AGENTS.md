# aave.watch

Aave, live: a read-only site that shows Aave v4 (and v3 totals) to anyone who uses Aave, from the
curious holder to the risk analyst. Who it is for and how it speaks: [PRODUCT.md](PRODUCT.md).

## Read before you change anything

| Doc | Read it when you touch |
|---|---|
| [docs/protocol.md](docs/protocol.md) | contract addresses, ABIs, events, view calls |
| [docs/architecture.md](docs/architecture.md) | repo layout, indexer, API, ClickHouse, Dragonfly, Worker |
| [docs/data-pipeline.md](docs/data-pipeline.md) | any table, key, stream, migration, indexer stage or price source |
| [docs/api.md](docs/api.md) | any endpoint, the stream, `crates/api`, `apps/worker`, any fetch in `apps/web` |
| [docs/dashboard-data.md](docs/dashboard-data.md) | any number shown in the UI, any API endpoint, any query |
| [docs/branding.md](docs/branding.md) | any UI, colour, font, copy or chart |
| [docs/visual-identity.html](docs/visual-identity.html) | any component: copy its markup, tokens and states from here |
| [PRODUCT.md](PRODUCT.md) | users, purpose, brand commitments, what must not be invented |
| [README.md](README.md) | project entry point; keep its doc list in sync with this table |

The docs are the spec. If code and a doc disagree, fix one of them in the same change. A new doc
gets a row here and a line in the README.

## Stack

- `apps/web`: Vite + React. `apps/worker`: Cloudflare Worker (API proxy, SSE relay, static assets, share images, link-preview tags).
- `crates/indexer`, `crates/api`, `crates/protocol`: Rust.
- ClickHouse for storage, Dragonfly for latest state, streams and cache.
- pnpm workspaces for `apps/*`, a Cargo workspace for `crates/*`.

## Rules

- State comes from contract views at a block; activity comes from events. Do not rebuild balances
  from events.
- Every state number carries its block in the data and the API. The UI shows it behind "Show blocks".
- The UI says pool, market, limit, bad debt; the code, the API and the data docs keep Hub, Spoke,
  cap, deficit. The word list is in `docs/branding.md`.
- A token logo is looked up by contract in `apps/web/public/icons/tokens.json`. Listing a new token
  adds its logo there in the same change, as SVG or PNG: the Worker draws the token's share image
  from it on request.
- Share images and link-preview text (title, description) of every page are written in
  `apps/worker/src/share.ts`. A new kind of page gets its entry there; check it with
  `pnpm --filter worker test:share`.
- Design tokens are the CSS variables in `apps/web/src/tokens.css`.
- UI uses the tokens in `docs/branding.md` only. No Aave logos or proprietary Aave fonts.
- `docs/branding.md` and `docs/visual-identity.html` change together.
- Preview the identity page with the `docs` server in `.claude/launch.json`.
- Status: `apps/web` is rebuilt on `PRODUCT.md`, `docs/branding.md`, `docs/visual-identity.html`
  and the page list in `docs/dashboard-data.md` (Now, Rates, Close calls, Token, Pool, Market,
  Wallet, search). What is still missing is under "Build order" in `docs/dashboard-data.md`.
- Status: the whole pipeline runs on all four chains: logs, view calls, the processor and the live
  tip. What is simplified or missing is listed under "Built so far" in docs/data-pipeline.md.
