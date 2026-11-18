# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Vite + React for the dashboard (`apps/web`), a Cloudflare Worker in front, Rust services behind it.
Chosen by the owner. Detail in [docs/architecture.md](docs/architecture.md).

## Users

A risk analyst watching Aave v4. They sit with the tool open for long stretches, scan tables, sort,
compare, and export. They know the protocol's terms and distrust a number they cannot trace.

## Product Purpose

aave.watch monitors Aave v4 in real time: liquidity across Hubs and Spokes, positions and their
health, caps, configuration changes, liquidations and deficit. Success is an analyst seeing a risk
form before it is realised, and being able to point at the block that shows it.

## Positioning

The pages are built around what only v4 has: the Hub-to-Spoke liquidity map, per-Spoke caps, the
base and premium rate split, per-position config versions, explicit deficit. Every state number
carries its block and age. Detail in [docs/dashboard-data.md](docs/dashboard-data.md).

## Operating Context

Desktop first, wide tables, used next to block explorers and governance forums. State is read from
contract views at a block; activity comes from events.

## Capabilities and Constraints

- Read-only. No wallet connection, no transactions, no accounts.
- Multi-chain from the start: Ethereum, Arc, Avalanche and Base.
- Alerts are not in the first version.
- Protocol terms keep the protocol's spelling: Hub, Spoke, reserve, drawn, premium, deficit.
- Undecided: risk thresholds, one USD price per Hub asset, history depth.

## Brand Commitments

- Name: aave.watch. Text wordmark, no symbol.
- Independent project. Every page footer says it is not affiliated with Aave Labs or the Aave DAO.
- No Aave logo, ghost mark, GHO mark, or proprietary Aave typefaces.
- Visual system: [docs/branding.md](docs/branding.md), shown in
  [docs/visual-identity.html](docs/visual-identity.html).
- Voice: plain and exact, numbers before adjectives.

## Evidence on Hand

- Real: Hub and Spoke names and addresses, event and view names ([docs/protocol.md](docs/protocol.md)).
- Absent: users, usage figures, testimonials, partner logos. Do not invent them. Sample data in any
  mock-up is labelled as sample data.

## Product Principles

1. A number without its block is not shown.
2. State from views, activity from events.
3. Tables before charts.
4. Colour means status or selection, nothing else.
5. Say the number, not the adjective.

## Accessibility & Inclusion

WCAG 2.2 AA. Status is never carried by colour alone. Full keyboard use. Reduced motion respected.
