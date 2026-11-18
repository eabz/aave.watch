# aave.watch

Monitoring and real-time app for Aave v4.

- [docs/protocol.md](docs/protocol.md): what we read from Aave v4 (contracts, addresses, events, views).
- [docs/architecture.md](docs/architecture.md): the monorepo, the services, and how data flows.
- [docs/data-pipeline.md](docs/data-pipeline.md): raw, parsed and enriched data; the ClickHouse schema and the
  Dragonfly keys.
- [docs/dashboard-data.md](docs/dashboard-data.md): what the dashboard shows and where each number comes from.
- [docs/branding.md](docs/branding.md): colours, type, shape and voice of the site.
- [docs/visual-identity.html](docs/visual-identity.html): the branding shown working. Every component
  in every state, both themes. Open it in a browser.
- [PRODUCT.md](PRODUCT.md): who the product is for and what it must never claim.

For contributors and coding agents: [AGENTS.md](AGENTS.md) (also loaded by [CLAUDE.md](CLAUDE.md)).

Status: scaffold only. `cargo build` for `crates/*`; `pnpm install`, then `pnpm --filter web build` and `pnpm --filter worker test` for `apps/*`.
