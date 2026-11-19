# Dashboard data

Draft for discussion. Every number below names its definition and its source, so we can argue about
the number before writing a query. Terms and contract names are in [protocol.md](protocol.md).

## Readers

Anyone who uses Aave: the curious, the depositor and the borrower, the analyst
([PRODUCT.md](../PRODUCT.md)). Decided 2026-11-19; from 2026-11-18 the only reader was a risk
analyst. Consequences:

- Every page opens with the answer to an everyday question; the dense panels sit one click down.
- The UI says pool, market, limit, bad debt, distance from liquidation. This file keeps the
  protocol's words (Hub, Spoke, cap, deficit, health factor) because it defines the numbers. The
  word list is in [branding.md](branding.md).
- Every number still has its block and age; the UI shows them behind "Show blocks".
- Per-user reads (health factor, positions) stay in the first build: the wallet page and Close calls
  depend on them.

## What only v4 has

A v3 dashboard is a list of markets. v4 splits a market in two, and the interesting data sits in the
split:

1. **Liquidity is shared, risk is not.** One Hub asset funds many Spokes. The question "who supplies
   this USDC and who borrows it" has a Spoke-by-Spoke answer.
2. **Caps are per Spoke per asset.** Each Spoke has its own add cap and draw cap on each Hub asset.
   Headroom is a number per (Hub, asset, Spoke).
3. **Borrowers of the same asset pay different rates.** Base rate comes from the Hub; the premium
   comes from each user's collateral quality.
4. **Config is versioned per position.** Users on the same reserve can sit on different collateral
   factors (dynamic config keys).
5. **Bad debt is explicit.** Deficit is reported per Spoke and per asset, on chain.

These five drive the pages that no other dashboard has: the liquidity map, cap headroom, premium
breakdown, config drift, deficit.

## Rules for all numbers

- **Three levels, never summed across levels.** Hub asset totals = sum of its Spokes = sum of their
  users. Protocol totals are taken at Hub level only.
- **State comes from views, activity from events.** Totals, rates and positions are contract reads
  at a block. Volumes and counts are aggregates over `events`.
- **Every state number carries its block and age.** A health factor read 40 minutes ago says so.
- **USD.** Oracles are per Spoke, so one asset can have several prices. Spoke and user numbers use
  that Spoke's oracle (it is the price that liquidates them). Hub and protocol numbers need one
  price per asset: *open decision 1*.
- **Supply APY is measured, not modelled.** `share price = getAddedAssets / getAddedShares`; APY is
  its growth over 24h and 7d, annualized. It includes premium income, fees and deficits without us
  re-deriving the contract math. A modelled "current" APY (`drawnRate × usage × (1 − liquidityFee)`,
  plus premium) is shown next to it, labelled as an estimate.
- **Borrow rate is two numbers.** Base (`drawnRate`) and effective (`drawnRate × (1 + riskPremium)`).
  For a Spoke, average risk premium on an asset = `premiumShares / drawnShares` from the Hub's
  `SpokeData`.

## Pages

Three menu pages, a search box, and one page for every token, pool, market and wallet. Each row
names where its numbers are defined below. Refresh: **live** = pushed over the stream as it
changes, **5m**, **1h** = cached query.

| Page | Opens with | Then | Numbers from |
|---|---|---|---|
| **Now** (`/`) | Deposited, borrowed, lent out, v4 share of Aave, wallets close to liquidation, each with its change over 7 days | Deposits and borrows over time; deposits by chain; v4's share of Aave over time (of deposits and of loans) and v3 next to v4 on every chain v3 runs on; big moves (the largest recent Supply, Withdraw, Borrow, Repay, LiquidationCall); the biggest assets with their rates | 1, 7 |
| **Rates** (`/rates`) | Every asset in every pool: deposited, earn (7 days), borrow rate, lent out | Filter by kind of asset and by chain; the list of pools | 2 |
| **Close calls** (`/close-calls`) | Wallets close to liquidation and those that can be liquidated now, biggest loans first | Recent liquidations; limits nearly full; anything paused, frozen or halted; bad debt; the list of markets | 5, 6 |
| **Search** | Tokens, pools, markets, wallets, in that order; the biggest of each before a word is typed | A pasted address opens the wallet | registry |
| **Token** (`/token/<symbol>`) | Deposited and borrowed across every pool and chain, the best place to earn, the cheapest place to borrow | Rate and deposits over time per pool; the pools that hold it; the markets that borrow it and how much of each limit is used; biggest holders (`/v1/holders`); the token's moves (`/v1/events?symbol=`) | 2, 3, 7 |
| **Pool** (`/pool/<chain>/<hub>`) | Deposited, borrowed, lent out, bad debt | What it is made of by kind of asset; every asset with its rates; the markets plugged into it; deposits and borrows over time; fees | 2, 3 |
| **Market** (`/market/<chain>/<spoke>`) | Deposited, borrowed, wallets close, wallets that can be liquidated | Loans by distance from liquidation; what can be deposited and borrowed, with the rules; biggest loans to watch; liquidations in this market | 4, 5, 6 |
| **Wallet** (`/wallet/<address>`) | The gauge: how far the wallet's closest loan is from liquidation | The wallet over time in its closest market (distance from liquidation, borrowed, deposited, from `/v1/history/account`: one reading a day before 2026-11-18, hourly or faster after); what it holds and owes in each market; its moves | 8 |

Not on a page yet: the liquidity map (3) beyond the per-token and per-pool tables, price shock,
stale risk premium, config drift and exit liquidity (5), and config changes (9). They are the
analyst's drill-downs and come after the pages above.

Distance from liquidation = 1 − 1 / health factor, as a percentage. Bands: Safe over 33% (HF ≥ 1.5),
Watch 9% to 33% (HF 1.1 to 1.5), Close under 9% (HF 1.0 to 1.1), can be liquidated now (HF < 1).
Placeholders: open decision 2.

"Lent out" is the usage ratio of a Hub asset. "Limit used" is `drawn / drawCap` (or
`supplied / addCap`) of one Spoke on one Hub asset. They are different numbers and are never both
called "how full".

## Number definitions

The panels below are grouped by subject, not by page. A page takes what its row above names.

### 1. Overview

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| Total supplied | Σ Hub assets `getAddedAssets` × price | `hub_asset_state` | live |
| Total borrowed | Σ `getAssetTotalOwed` × price (drawn + premium) | `hub_asset_state` | live |
| Available liquidity | Σ `liquidity` × price; swept shown apart | `hub_asset_state` | live |
| Supplied / borrowed over time | same, daily and hourly, with the change per step (Adoption page) | rollup of `hub_asset_state` | 1h |
| v4 share of Aave | v4 / (v3 + v4), supplied and borrowed, over time and per chain (Adoption page) | rollup of `hub_asset_state`, `v3_reserve_state` | 1h |
| Split by Hub, split by Spoke | same totals grouped | `hub_asset_state`, `hub_spoke_state` | 5m |
| 24h volume by action | Σ USD of Supply, Withdraw, Borrow, Repay, LiquidationCall | `events` | 5m |
| Active users 24h / 7d / 30d | distinct `user` in Spoke activity events | `events` | 1h |
| Liquidations 24h | count, debt repaid, collateral seized | `events` | live |
| Protocol revenue | Σ `MintFeeShares.assets` × price, plus accrued not yet minted | `events`, `hub_asset_state` | 1h |
| Outstanding deficit | Σ `deficitRay` × price | `hub_asset_state` | live |
| Positions at risk | count and debt with HF < 1.1 | `user_account_state` | 5m |

### 2. Hub and Hub asset

Hub page is a table of its assets; each row opens the asset.

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| Supplied, drawn, premium owed, idle, swept | `getAddedAssets`, `getAssetOwed`, `liquidity`, `swept` | `hub_asset_state` | live |
| Usage ratio | `drawn / (liquidity + drawn + swept)` | `hub_asset_state` | live |
| Base borrow rate | `drawnRate` | `Hub.UpdateAsset` | live |
| Supply APY | measured 24h / 7d, plus estimate | `hub_asset_state` | 1h |
| Rate curve | optimal usage, base rate, two slopes, with the current point on it | `UpdateInterestRateData`, `hub_asset_state` | on change |
| Usage and rate history | time series | `hub_asset_state` | 1h |
| Spoke allocation | per Spoke: supplied, drawn, premium, share of total | `hub_spoke_state` | live |
| Cap headroom | per Spoke: supplied / addCap, drawn / drawCap | `hub_spoke_state` | live |
| Spoke status | active, halted, risk premium threshold and how close the Spoke is to it | `hub_spoke_state` | live |
| Fees | liquidity fee, accrued fees, minted fee shares over time | `hub_asset_state`, `events` | 1h |
| Reinvestment | swept now, sweep and reclaim history, controller address | `hub_asset_state`, `events` | on change |
| Deficit | per Spoke now, reported and eliminated over time | `hub_spoke_state`, `events` | live |

### 3. Liquidity map

One view for the whole protocol: Hub assets on one side, Spokes on the other.

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| Flow diagram | for each (Hub asset, Spoke): supplied in, drawn out | `hub_spoke_state` | 5m |
| Net position | per Spoke per asset: supplied − drawn. Shows which Spokes fund which | `hub_spoke_state` | 5m |
| Cap matrix | grid of (asset, Spoke) coloured by cap usage | `hub_spoke_state` | 5m |
| Net flow 24h / 7d | Σ `Add − Remove` and `Draw − Restore` per (asset, Spoke) | `events` | 1h |

### 4. Spoke and reserve

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| Supplied, borrowed | `getReserveSuppliedAssets`, `getReserveTotalDebt` × Spoke price | `reserve_state` | live |
| Reserve table | per reserve: Hub and asset, supplied, debt, price, collateral factor, collateral risk, max liquidation bonus, liquidation fee, flags (paused, frozen, borrowable) | `reserve_state`, registry | live |
| Borrow rates | base, average effective (base × (1 + avg risk premium)) | `hub_asset_state`, `hub_spoke_state` | live |
| Liquidation settings | target HF, HF for max bonus, bonus factor | `UpdateLiquidationConfig` | on change |
| Oracle | price, source address, last change, price history | `prices`, `events` | live |
| Users | suppliers, borrowers, new per day | `events`, `user_account_state` | 1h |
| Health factor distribution | debt (USD) by HF bucket | `user_account_state` | 5m |
| Risk premium distribution | debt (USD) by user risk premium bucket | `user_account_state` | 1h |
| Top positions | largest by collateral and by debt | `user_account_state` | 5m |
| Concentration | share of supply and of debt held by top 10 users | `user_reserve_state` | 1h |

### 5. Risk

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| At-risk positions | HF < 1.1: user, Spoke, collateral, debt, HF, age of the reading | `user_account_state` | live |
| Liquidatable now | HF < 1, with debt to repay to reach target HF | `user_account_state` | live |
| Price shock | debt that falls below HF 1 if asset X drops 10 / 20 / 30% | `user_reserve_state`, `prices`, reserve config | 5m |
| Stale risk premium | users whose stored premium (`getUserLastRiskPremium`) differs from the current computed one; the gap is interest not being charged | `user_account_state` | 1h |
| Config drift | debt and collateral on positions bound to an old dynamic config key, per reserve | `user_reserve_state`, registry | 1h |
| Collateral behind debt | per borrowed asset and Spoke: which collaterals back it, in USD | `user_reserve_state` | 1h |
| Exit liquidity | per Hub asset: idle liquidity vs the supply of its top 10 suppliers; usage above optimal | `hub_asset_state`, `user_reserve_state` | 5m |
| Oracle divergence | same asset priced differently across Spokes; prices unchanged for too long | `prices` | live |
| Cap pressure | (asset, Spoke) pairs above 90% of add or draw cap | `hub_spoke_state` | live |
| Paused, frozen, halted | everything currently restricted | `reserve_state`, `hub_spoke_state` | live |
| Deficit | per Hub, asset, Spoke; history | `hub_spoke_state`, `events` | live |

Price shock is computed by us from stored positions; it is an approximation of the contract's math
and is labelled as one.

### 6. Liquidations

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| Log | each `LiquidationCall`: user, liquidator, pair, debt repaid, collateral seized, tx | `events` | live |
| Bonus paid | collateral seized (USD) − debt repaid (USD) | `events` | live |
| Protocol fee | `collateralSharesLiquidated − collateralSharesToLiquidator`, in USD | `events` | live |
| Deficit created | `ReportDeficit` in the same tx | `events` | live |
| Volume over time, by Spoke and by collateral | daily Σ | `events` | 1h |
| Liquidators | ranking by volume and by bonus earned | `events` | 1h |

### 7. Live feed

Every decoded event as it lands: type, Hub or Spoke, asset, user, amount, USD, tx, block. Filters:
type, Hub, Spoke, asset, user, minimum USD. Source: `feed` stream; scroll-back from `events`.
Events from a block that is later reorged are removed from the list.

### 8. Wallet

Input: an address. One block per Spoke where it has a position.

| Panel | Definition | Source | Refresh |
|---|---|---|---|
| Account | HF, collateral, debt, risk premium, average collateral factor | `user_account_state`, read fresh on open | live |
| Reserves | supplied, used as collateral, drawn debt, premium debt, collateral factor it is bound to | `user_reserve_state` | live |
| Rates | base and effective borrow rate per debt; supply APY per supply | derived | live |
| Liquidation distance | price drop of each collateral that takes HF to 1 | derived | 5m |
| History | the address's events; HF and risk premium over time | `events`, `user_account_state` | 5m |
| Position managers | approved managers; actions where caller ≠ user | `events` | 5m |

### 9. Config changes

A timeline of every governance and risk change, each with old value and new value: asset config,
Spoke caps and flags, reserve config, dynamic config, liquidation config, price sources, rate
curves, new assets, Spokes and reserves, position managers. Source: `events`. Refresh: live.

## Build order

The data for all of this is already indexed and served ([api.md](api.md)); what follows is the
order of the UI rebuild.

1. The shell: header, search, footer, tokens of [branding.md](branding.md), logos.
2. Now.
3. Token, Pool, Market, Wallet.
4. Rates and Close calls.
5. The analyst's drill-downs listed under "Not on a page yet".

Built 2026-11-19: steps 1 to 4, the wallet's history over time, biggest holders and moves of a
token, a pool's totals over time (`/v1/history/totals?hub=`), sortable columns, and the live stream:
one connection to `/v1/stream` (`feed`, `risk`) adds moves to Now as they happen and makes Now,
Close calls, a market and a wallet read their figures again, at most every five seconds. Polling
every 15 to 60 seconds stays as the fallback.

## Open decisions

1. **One USD price per Hub asset.** Options: median of the Spokes' oracle prices for that asset
   (proposed), the price from the Spoke with the most supply, or an outside feed.
2. **Risk thresholds.** HF 1.1 for "at risk", 90% for cap pressure, and the age at which a price
   counts as stale are placeholders.
3. **Non-standard Spokes.** Treasury, Tokenization and Babylon Spokes do not have the standard user
   model. Proposed: show them in Hub allocation from day one, give them their own Spoke page later.
4. **History depth.** Hourly state forever, or hourly for 90 days and daily after.
5. **Wallet labels.** Naming known addresses (liquidators, treasuries, managers) needs a list we
   maintain by hand.
