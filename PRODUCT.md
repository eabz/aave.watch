# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Vite + React for the site (`apps/web`), a Cloudflare Worker in front, Rust services behind it.
Chosen by the owner. Detail in [docs/architecture.md](docs/architecture.md).

## Users

Anyone with a reason to look at Aave. Decided 2026-11-19; before that the only reader was a risk
analyst.

- **The curious.** Holds AAVE or follows the protocol. Asks: how big is v4, is it growing, is it
  taking over from v3?
- **The depositor and the borrower.** Has money in Aave or is about to. Asks: what does my asset
  earn, what does it cost to borrow, am I close to liquidation?
- **The analyst.** Watches risk for a living. Knows the protocol's terms and distrusts a number they
  cannot trace. Asks: which limits are nearly full, which loans are close, where is the bad debt?

The first two decide what a page opens with. The third is served one click down, never by making
the first screen denser.

## Product Purpose

aave.watch shows Aave live: how much is in it, what each asset earns and costs, who is close to
liquidation, and what just happened. Success is a person getting their answer from the first screen
in everyday words, and an analyst still being able to point at the block that proves it.

## Positioning

"Aave, live." The official apps are where you deposit and borrow. aave.watch is where you look: one
page for every token, pool, market and wallet, reachable from one search box, each opening with the
answer to a question a person would ask. Detail in [docs/dashboard-data.md](docs/dashboard-data.md).

## Operating Context

Phone and desktop alike: checked for a minute from a link someone shared, or left open next to a
block explorer. State is read from contract views at a block; activity comes from events.

## Capabilities and Constraints

- Read-only. No wallet connection, no transactions, no accounts.
- Aave v4 on Ethereum, Arc, Avalanche and Base, plus Aave v3 totals on every chain for comparison.
- Three menu pages (Now, Rates, Close calls) and a global search. Tokens, pools, markets and wallets
  each have their own page.
- Alerts are not in the first version.
- Undecided: risk thresholds, one USD price per Hub asset, history depth.

## Brand Commitments

- Name: aave.watch, written as one lowercase word. Mark: the Tick.
- Independent project. Every page footer says it is not affiliated with Aave Labs or the Aave DAO.
- No Aave logo, ghost mark, GHO mark, or proprietary Aave typefaces. No mark of ours with eyes or a
  ghost: Aave's own mark is an arch with two eyes.
- Token and chain logos identify the token or chain and nothing else. The AAVE and GHO token logos
  appear only as those tokens' logos, never as ours.
- Visual system: [docs/branding.md](docs/branding.md), shown in
  [docs/visual-identity.html](docs/visual-identity.html).
- Voice: the friend who watches Aave for you. Everyday word first, exact number always.

## Evidence on Hand

- Real: every figure on the site, read from the chain; Hub and Spoke names and addresses
  ([docs/protocol.md](docs/protocol.md)).
- Absent: users, usage figures, testimonials, partner logos, names for wallets. Do not invent them.
  Sample data in any mock-up is labelled as sample data.

## Product Principles

1. Answer first: a question, one number, then the detail.
2. The everyday word first; the protocol's word one click down.
3. Every number can show its block. It does not have to shout it.
4. State from views, activity from events.
5. Colour means a kind of thing or a state, and always comes with a word or an icon.
6. Funny only when it is quiet. Never about someone losing money, never in a number.

## Accessibility & Inclusion

WCAG 2.2 AA. Nothing is carried by colour alone. Full keyboard use. Reduced motion respected.
