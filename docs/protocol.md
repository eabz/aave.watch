# Aave v4: what we read

Source of truth: [aave/aave-v4](https://github.com/aave/aave-v4) at commit `4885a7a` (2026-11-14) and
[aave.com/docs/aave-v4](https://aave.com/docs/aave-v4). Event and struct definitions below are copied
from the interfaces at that commit. Re-check them when the repo moves.

## Model in one page

- **Hub**: contract that holds the liquidity (deployed behind a proxy, see "ABIs"). It lists **assets** (`assetId`, sequential
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

Hubs on all four chains, from DefiLlama's adapter (`DefiLlama-Adapters/projects/aave-v4/index.js`).
Each was checked on chain on 2026-11-18: it answers `getAssetCount()`. The machine-readable copy is
[registry/chains.toml](../registry/chains.toml).

| Chain | Hub | Address | Assets | First block |
|---|---|---|---|---|
| Ethereum (1) | Core | `0xCca852Bc40e560adC3b1Cc58CA5b55638ce826c9` | 18 | 24,720,891 |
| Ethereum (1) | Plus | `0x06002e9c4412CB7814a791eA3666D905871E536A` | 7 | 24,720,895 |
| Ethereum (1) | Prime | `0x943827DCA022D0F354a8a8c332dA1e5Eb9f9F931` | 7 | 24,720,887 |
| Ethereum (1) | Global Dollar | `0x62d63197660c080236193CA60b70E49A08E90368` | 7 | 25,318,132 |
| Base (8453) | Equities | `0xa4d5947Eb727A052bae69C593FfC84247EC9864E` | 9 | 51,478,826 |
| Avalanche (43114) | Core | `0xd07369fAE4A5BB13c9Ce446B052c7867B1AbDf6e` | 8 | 89,721,368 |
| Arc (5042) | Core | `0x17288dfc86205301064577b98B02b81017e6F79C` | 5 | 18,791,506 |

Spokes are read from the Hubs (`getSpokeCount`, `getSpokeAddress` per asset), not listed by hand:
55 on Ethereum, 3 on Base, 12 on Avalanche, 8 on Arc. Several Ethereum Spokes draw from more than
one Hub; the Treasury Spoke is connected to all four.

Every standard and Treasury Spoke has a published name. Names are from the Aave docs address page
([aave.com/docs/resources/addresses](https://aave.com/docs/resources/addresses), read 2026-11-19),
which agrees with the `@aave-dao/aave-address-book` constants (4.77.8). "Gold Correlated" is not in
the docs: its name is from the Aave Pro API (`spokes.name` at `api.aave.com/internal/v4/graphql`).
The last column is the name Aave Pro shows where it differs from the docs. Tokenization Spokes are
named by their vault symbol.

| Chain | Spoke | Address | Aave Pro shows |
|---|---|---|---|
| Ethereum | Main | `0x94e7A5dCbE816e498b89aB752661904E2F56c485` | |
| Ethereum | Bluechip | `0x973a023A77420ba610f06b3858aD991Df6d85A08` | |
| Ethereum | Ethena Correlated | `0x58131E79531caB1d52301228d1f7b842F26B9649` | |
| Ethereum | Ethena Ecosystem | `0xba1B3D55D249692b669A164024A838309B7508AF` | |
| Ethereum | EtherFi eSpoke | `0xbF10BDfE177dE0336aFD7fcCF80A904E15386219` | Etherfi |
| Ethereum | Forex | `0xD8B93635b8C6d0fF98CbE90b5988E3F2d1Cd9da1` | |
| Ethereum | Gold | `0x65407b940966954b23dfA3caA5C0702bB42984DC` | |
| Ethereum | Gold Correlated | `0x115aF1C2db26cD8ECa3f29055E4E424AF4edF616` | (address book: `PAXG_PAXOS_ESPOKE`) |
| Ethereum | Kelp eSpoke | `0x3131FE68C4722e726fe6B2819ED68e514395B9a4` | Kelp |
| Ethereum | Lido eSpoke | `0xe1900480ac69f0B296841Cd01cC37546d92F35Cd` | Lido |
| Ethereum | Lombard BTC | `0x7EC68b5695e803e98a21a9A05d744F28b0a7753D` | Lombard |
| Ethereum | PAXG Gold | `0xAD75cE6354f87F3135cE10621d385d8D1e2562C2` | |
| Ethereum | USDG Maple eSpoke | `0x774b9655413c34809c1f1b16b654465A89EBE989` | Maple SyrupUSDG |
| Ethereum | USDG Pendle | `0x956d8e0A89cfa3744428C4641b5a53B56167a7f9` | |
| Ethereum | Treasury | `0xB9B0b8616f6Bf6841972a52058132BE08d723155` | |
| Base | MAG7 | `0x17905Db0e4A3514467539956c084180616AE7B8D` | Coinbase Stocks |
| Base | Treasury | `0x5F8d0102F5B51Fae6DE9d2F2561bda63Fb5Db674` | |
| Avalanche | Main | `0x435272CefF93a1E657E8ABfdf0A13e95900A3a56` | |
| Avalanche | AVAX Correlated | `0x3b517594277c67307CF2d7CBE6FE1D4399B68c41` | |
| Avalanche | Ethena Ecosystem | `0xaf22446124e566e54845f65b722E4dCbf12b650A` | |
| Avalanche | Forex | `0x6a37776B5E026dBdF043b4F933c323C84DD1B514` | |
| Avalanche | Treasury | `0x2C4Aea1A5F000889c6DfFE8f52377aFc2CB113a6` | |
| Arc | Main | `0xB843bdC3a87A05E77E07Df9FE48928b3A34b134d` | |
| Arc | Forex | `0x4164EBCAF74670aa74C8D4F59de6157c0780F1bB` | |
| Arc | USDC Maple eSpoke | `0x18Dde098d25722C14e09842a6Fa7db6aAFC395a2` | Maple SyrupUSDC |
| Arc | Treasury | `0xcbd466CB8709D9f6dd8312668B4dbef394cE0e15` | |

### Spoke kinds

Every address a Hub lists as a Spoke was probed on chain on 2026-11-18. The full list, with kind,
Hubs, oracle and name, is [registry/spokes.toml](../registry/spokes.toml).

| Kind | What it is | How it is recognised | Ethereum | Base | Avalanche | Arc |
|---|---|---|---|---|---|---|
| `spoke` | standard lending Spoke (`ISpoke`) | answers `getReserveCount()` and `ORACLE()` | 14 | 1 | 4 | 3 |
| `tokenization` | ERC-4626 vault over one Hub asset, e.g. "Wrapped Aave Core USDC" | answers `asset()` and `symbol()` | 40 | 1 | 7 | 4 |
| `treasury` | receives protocol fees for every Hub on its chain | answers `owner()`, none of the above | 1 | 1 | 1 | 1 |

So 52 of the 78 Spokes are vaults: users of a tokenization Spoke hold vault shares and have no
position, health factor or liquidation. Risk pages are about the 22 standard Spokes; the vaults and
the Treasury matter for Hub allocation and supply.

Each standard Spoke has its own `AaveOracle` (22 in total), recorded in the registry.

## ABIs

The ABIs in `crates/protocol/abi/` are those of the **deployed** contracts, as verified on Sourcify,
fetched 2026-11-18. `topics.tsv` there lists every event with its topic0.

| File | Contract | Events |
|---|---|---|
| `hub.json` | `HubInstance` | 18 |
| `spoke.json` | `SpokeInstance` | 22 |
| `tokenization_spoke.json` | `TokenizationSpokeInstance` | 6 |
| `treasury_spoke.json` | `TreasurySpokeInstance` | 3 |
| `aave_oracle.json` | `AaveOracle` | 2 |
| `interest_rate_strategy.json` | `AssetInterestRateStrategy` | 1 |
| `access_manager.json` | `AccessManagerEnumerable` (the `authority()` of Hubs and Spokes) | 12 |
| `chainlink_aggregator_proxy.json` | `EACAggregatorProxy`, the price source of ETH / USD on the Main Spoke | 4 |

What was checked:

- **One ABI per kind, on every chain.** The seven Hub implementations have byte-identical code. The
  implementations of each Spoke kind have the same code size on all four chains (they differ only
  in constructor constants), so one ABI serves each kind. Base and Arc contracts are not on
  Sourcify; they are covered by this comparison, not by their own verification.
- **Events match live logs.** Recent logs of the Ethereum Core Hub (584), the Ethereum Main Spoke
  (381) and the Avalanche Core Hub (204) contain no topic0 outside these ABIs.
- **Against the repo.** The deployed event set equals the one listed below from `aave/aave-v4`, plus
  two from the proxy and access layers: `Initialized` and `AuthorityUpdated`.

Two corrections to what the Aave docs say:

- **Hubs and Spokes are behind proxies.** Every Hub and Spoke address is an ERC-1967 proxy; the code
  sits in an implementation (`HubInstance`, `SpokeInstance`, ...). The docs call the Hub immutable.
  An upgrade can change the event set. The proxies' `Upgraded` event is parsed like any other and
  shows in the config timeline; parsing is not stopped by it, so review the ABIs when one appears.
  Raw logs keep flowing either way.
- **Tokenization Spoke events** are the ERC-4626 and ERC-20 ones: `Deposit`, `Withdraw`, `Transfer`,
  `Approval`, plus `SetTokenizationSpokeImmutables`. The Treasury Spoke emits only ownership events.

Not known yet:

- Names for 12 standard Spokes (the rest are named: docs, or the vault symbol).
- Addresses of configurators and position managers.
- A Babylon Spoke: the code exists in the repo, none is connected to a Hub yet.

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
