# Aave v4: what we read

Source of truth: [aave/aave-v4](https://github.com/aave/aave-v4) at commit `4885a7a` (2026-11-14) and
[aave.com/docs/aave-v4](https://aave.com/docs/aave-v4). Event and struct definitions below are copied
from the interfaces at that commit. Re-check them when the repo moves.

## Model in one page

- **Hub**: immutable contract that holds the liquidity. It lists **assets** (`assetId`, sequential
  from 0), sets the interest rate per asset, and gives each connected Spoke an add cap and a draw cap.
- **Spoke**: upgradeable contract users talk to. It lists **reserves** (`reserveId`), each one pointing
  at a `(hub, assetId)`. Risk settings, oracle and liquidation rules are per Spoke. One Spoke can use
  several Hubs.
- **Debt has two parts.** *Drawn debt* is principal plus base interest at the Hub's drawn rate.
  *Premium debt* is extra interest from the user's **risk premium**, which depends on the quality
  (`collateralRisk`) of the collateral covering the debt. A borrower's effective rate is
  `drawnRate × (1 + riskPremium)`.
- **Usage ratio** (utilization) is `drawn / (liquidity + drawn + swept)`
  (`AssetInterestRateStrategy.sol:117`). `swept` is liquidity sent to a reinvestment controller.
- **Dynamic config**: collateral factor, max liquidation bonus and liquidation fee live under a
  `dynamicConfigKey` per reserve. A position keeps the key it was last bound to, so two users of the
  same reserve can be on different collateral factors.
- **Liquidation**: allowed below health factor 1. Repays only enough to reach the Spoke's
  `targetHealthFactor`. Bonus grows as health factor falls (max at `healthFactorForMaxBonus`).
  Unrecoverable debt is reported to the Hub as **deficit**.

## Deployments

Ethereum mainnet (chain 1), from the docs. Checked on chain on 2026-11-18: each Hub answers
`getAssetCount()`, the Main Spoke answers `getReserveCount()`.

| Hub | Address | Assets |
|---|---|---|
| Core | `0xCca852Bc40e560adC3b1Cc58CA5b55638ce826c9` | 18 |
| Plus | `0x06002e9c4412CB7814a791eA3666D905871E536A` | 7 |
| Prime | `0x943827DCA022D0F354a8a8c332dA1e5Eb9f9F931` | 7 |

| Spoke | Address |
|---|---|
| Main | `0x94e7A5dCbE816e498b89aB752661904E2F56c485` |
| Bluechip | `0x973a023A77420ba610f06b3858aD991Df6d85A08` |
| Ethena Correlated | `0x58131E79531caB1d52301228d1f7b842F26B9649` |
| Ethena Ecosystem | `0xba1B3D55D249692b669A164024A838309B7508AF` |
| EtherFi eSpoke | `0xbF10BDfE177dE0336aFD7fcCF80A904E15386219` |
| Forex | `0xD8B93635b8C6d0fF98CbE90b5988E3F2d1Cd9da1` |
| Gold | `0x65407b940966954b23dfA3caA5C0702bB42984DC` |
| Kelp eSpoke | `0x3131FE68C4722e726fe6B2819ED68e514395B9a4` |
| Lido eSpoke | `0xe1900480ac69f0B296841Cd01cC37546d92F35Cd` |
| Lombard BTC | `0x7EC68b5695e803e98a21a9A05d744F28b0a7753D` |
| Treasury | `0xB9B0b8616f6Bf6841972a52058132BE08d723155` |

Not every Spoke is an `ISpoke`. The Treasury Spoke implements `ITreasurySpoke` (it reverts on
`getReserveCount()`), and the repo also has `TokenizationSpoke` and `BabylonSpoke` variants. The
indexer must know the kind of each Spoke.

Not known yet:

- Deployment block of each contract (needed as the backfill start).
- Avalanche addresses. News and a governance post say v4 is live there; the docs list only Ethereum.
- Addresses of oracles, configurators and position managers. Each Spoke exposes `ORACLE()`.

The seed list above is only a start. The chain is the registry: `Hub.AddSpoke` lists every Spoke,
`Spoke.AddReserve` lists every reserve.

## Events

### Hub (`IHub`, `IHubBase`)

| Event | Args | Use |
|---|---|---|
| `AddAsset` | assetId, underlying, decimals | registry |
| `AddSpoke` | assetId, spoke | registry |
| `UpdateAssetConfig` | assetId, config(feeReceiver, liquidityFee, irStrategy, reinvestmentController) | config log |
| `UpdateSpokeConfig` | assetId, spoke, config(addCap, drawCap, riskPremiumThreshold, active, halted) | config log, caps |
| `UpdateAsset` | assetId, drawnIndex, drawnRate, accruedFees | rate and index history |
| `Add` / `Remove` | assetId, spoke, shares, amount | Spoke supplies to / withdraws from Hub |
| `Draw` | assetId, spoke, drawnShares, drawnAmount | Spoke borrows from Hub |
| `Restore` | assetId, spoke, drawnShares, premiumDelta, drawnAmount, premiumAmount | Spoke repays Hub |
| `RefreshPremium` | assetId, spoke, premiumDelta | premium accounting |
| `ReportDeficit` | assetId, spoke, drawnShares, premiumDelta, deficitAmountRay | bad debt |
| `EliminateDeficit` | assetId, callerSpoke, coveredSpoke, shares, deficitAmountRay | bad debt covered |
| `MintFeeShares` | assetId, feeReceiver, shares, assets | protocol revenue |
| `Sweep` / `Reclaim` | assetId, reinvestmentController, amount | reinvestment |
| `TransferShares` | assetId, sender, receiver, shares | share moves between Spokes |

Interest rate strategy: `UpdateInterestRateData(hub, assetId, optimalUsageRatio, baseDrawnRate,
rateGrowthBeforeOptimal, rateGrowthAfterOptimal)`.

### Spoke (`ISpoke`)

| Event | Args | Use |
|---|---|---|
| `AddReserve` | reserveId, assetId, hub | registry |
| `UpdateReserveConfig` | reserveId, config(collateralRisk, paused, frozen, borrowable, receiveSharesEnabled) | config log |
| `AddDynamicReserveConfig` / `UpdateDynamicReserveConfig` | reserveId, dynamicConfigKey, config(collateralFactor, maxLiquidationBonus, liquidationFee) | config log |
| `UpdateLiquidationConfig` | config(targetHealthFactor, healthFactorForMaxBonus, liquidationBonusFactor) | config log |
| `UpdateReservePriceSource` | reserveId, priceSource | config log |
| `UpdatePositionManager` | positionManager, active | config log |
| `Supply` / `Withdraw` | reserveId, caller, user, shares, amount | activity |
| `Borrow` | reserveId, caller, user, drawnShares, drawnAmount | activity |
| `Repay` | reserveId, caller, user, drawnShares, totalAmountRepaid, premiumDelta | activity |
| `LiquidationCall` | collateralReserveId, debtReserveId, user, liquidator, receiveShares, debtAmountRestored, drawnSharesLiquidated, premiumDelta, collateralAmountRemoved, collateralSharesLiquidated, collateralSharesToLiquidator | liquidations |
| `ReportDeficit` | reserveId, user, drawnShares, premiumDelta | bad debt per user |
| `SetUsingAsCollateral` | reserveId, caller, user, usingAsCollateral | activity |
| `UpdateUserRiskPremium` | user, riskPremium | risk premium history |
| `RefreshAllUserDynamicConfig` / `RefreshSingleUserDynamicConfig` | user (, reserveId) | config rebinding |
| `RefreshPremiumDebt` | reserveId, user, premiumDelta | premium accounting |
| `SetUserPositionManager` | user, positionManager, approve | activity |

Oracle (`IAaveOracle`): `UpdateReserveSource(reserveId, source)`. Price changes emit nothing.

## Views we call

State is read from views, not rebuilt from events. Events tell us *when* to read.

- Hub, per asset: `getAsset` (the full struct: liquidity, swept, shares, drawnIndex, drawnRate,
  deficit, fees), `getAddedAssets`, `getAddedShares`, `getAssetTotalOwed`, `getAssetOwed`,
  `getAssetAccruedFees`, `getAssetConfig`.
- Hub, per asset and Spoke: `getSpoke` (caps, flags, shares, deficit), `getSpokeAddedAssets`,
  `getSpokeTotalOwed`, `getSpokeOwed`.
- Spoke, per reserve: `getReserve`, `getReserveConfig`, `getDynamicReserveConfig`,
  `getReserveSuppliedAssets`, `getReserveTotalDebt`, `getReserveDebt`, `getLiquidationConfig`.
- Spoke, per user: `getUserAccountData` (riskPremium, avgCollateralFactor, healthFactor,
  totalCollateralValue, totalDebtValueRay, activeCollateralCount, borrowCount), `getUserPosition`,
  `getUserSuppliedAssets`, `getUserTotalDebt`, `getUserReserveStatus`, `getUserLastRiskPremium`.
- Oracle: `getReservePrice`, `getReservesPrices`, `decimals`.

## Official data layer

Aave ships AaveKit (`@aave/client`, `@aave/react`), a GraphQL API with hubs, spokes and summary
history. We do not depend on it: it has no event stream and we need our own data for real time. It
is useful as a cross-check for our numbers.
