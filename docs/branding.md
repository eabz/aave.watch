# Branding

The visual system for aave.watch. Every rule here is shown working in
[visual-identity.html](visual-identity.html): open it in a browser to see each token and component
in both themes. A value that is not in this file does not go in the UI; if the two files disagree,
fix them in the same change.

Rewritten 2026-11-19 for the new direction ([PRODUCT.md](../PRODUCT.md)). The site in `apps/web`
still wears the old look until it is rebuilt on this file and on the identity page.

## Identity

aave.watch is where you look at Aave: warm, familiar, a little funny, and exact. It should feel at
home next to aave.com, Aave Pro and Uniswap's Explore page, and read as its own product.

- **Shared with the ecosystem:** rounded shapes, token and chain logos, everyday words for money.
- **Our own:** the pink accent, the Tick mark, the wallet gauge, the pills,
  "distance from liquidation" instead of a health factor.
- **Never used:** the Aave logo, the ghost mark, the GHO mark, Aave's proprietary typefaces (Aave
  Repro, Aave Repro Mono, Aave Aguzzo, FT Regola Neue), or any mark of ours with eyes or a ghost.

Every page footer carries: *"aave.watch is an independent project. Not affiliated with Aave Labs or
the Aave DAO."*

### Logo

- **Mark, the Tick:** a W for "watch" drawn as a price line, in `on-fill` on a rounded `accent`
  tile. On a 48 grid: tile radius 14, line through (11,17) (18,32) (24,21) (30,32) (37,15), stroke
  4.5 with round caps and joins. Below 20px the stroke is 6.
- **Live dot:** when the page is receiving live data, a dot at the end of the line pulses once
  every 2.4s. It is still when the data is stale or motion is reduced.
- **Wordmark:** `aave.watch` as one word: lowercase, the title face at weight 700, letter-spacing
  −0.045em, all in the text colour. The dot is not coloured. The gap between mark and word is a
  quarter of the mark's width.
- **Favicon:** the mark alone.

### Voice

The friend who watches Aave for you. Everyday word first, exact number always.

| We write | We do not write |
|---|---|
| $1.31B is sitting in Aave v4 | Supplied $1.31B |
| 0.3% from liquidation | HF 1.003 |
| USDC is nearly full: 93% of its limit is used | Draw cap 93.0% used: Main Spoke, USDC |
| All quiet in Gold. Nobody is close. | No positions below HF 1.1 in Gold Spoke. |
| Someone took out $788K of USDG | Withdraw 787,931 USDG, Main Spoke |
| Could not load this. Showing the reading from 2 min ago. Try again | Oops! Something went wrong |

- **Answer first.** A page opens straight on its answer: the stat strip, or the gauge on a wallet
  page. There is no headline sentence above it (the owner dropped it on 2026-11-19). A card title
  is a plain name or a question a person would ask.
- **Our word, then the protocol's.** The protocol's word appears once on the detail page, as a pill
  ("Pool (Hub)"), and in tooltips.

| Protocol | We say | Protocol | We say |
|---|---|---|---|
| Hub | pool | supplied / drawn | deposited / borrowed |
| Spoke | market | health factor | distance from liquidation |
| cap | limit | deficit | bad debt |
| usage | lent out | risk premium | extra rate for risk |
| cap usage | limit used | liquidation bonus | liquidator's bonus |
| collateral factor | counts for | liquidatable | can be liquidated now |

- **Funny only when it is quiet.** A joke may sit in an empty state or beside a harmless number
  ("Last liquidation: $2.54. Not a typo."). Never on a wallet that is close or liquidated, never in
  an error, never in a number.
- **Exact.** Real figures and real addresses.
- Errors say what failed, what the reader is looking at instead, and the one thing to do.
- No exclamation marks, no emoji, no "please". Sample data is always labelled as sample data.

Number formats: USD `$1.31B`, `$137.1M`, `$788K`, `$2.54`; exact USD in a wallet `$11,221,532`;
percent `4.96%`, `68%` in a bar; change of an amount `▲ 7.6%`, `▼ 2.1%`; change of a rate in points
`▼ 1.92 pts`, never a percent of a percent; block `21,480,112`; age `10 min ago`,
`1h 35m ago`, then a date `14 Nov`; address `0x1b36…1cd6`. One figure has one format: $137.1M on one
page is $137.1M on every page (B two decimals, M one, K none, under $1,000 to the cent). A value
too small to round is "under 0.01%", not `0.00%`.

**Freshness.** A page does not print when it was read (the owner dropped the "Live · read ..."
line on 2026-11-19). The dot on the mark pulses while the live stream is connected. Only when a
page's newest reading is over two hours old does it say so: "Stale · read 19 Nov 2026, 01:38 UTC",
with an icon in `amber`. "Show blocks", in the footer, puts the block number and its age under
every figure on the page and is remembered.

## Colour

The accent is pink, chosen 2026-11-19 over Aave's lilac, a lime and no accent at all, so the site
reads as its own product. Lilac stays, as the colour of ETH.

### Neutrals

| Token | Day | Night | Use |
|---|---|---|---|
| `bg` | `#f6f4fb` | `#121016` | page |
| `card` | `#ffffff` | `#1c1a22` | cards, tables, the search box |
| `card-2` | `#efecf8` | `#26232e` | bar tracks, the switch track, hover row |
| `line` | `#e2deee` | `#322e3d` | card edges, row lines, chart grid |
| `ink` | `#1d1a24` | `#f4f2f8` | text, figures |
| `ink-2` | `#5d586b` | `#aaa5b8` | labels, secondary text, axis ticks |

The neutrals lean violet, which sits quietly under the pink. Cards are always lighter than the page.

### Accent

| Token | Day | Night | Use |
|---|---|---|---|
| `accent` | `#ff5fa2` | `#ff5fa2` | the mark's tile |
| `accent-strong` | `#d92d78` | `#ff7ab3` | fills that carry a value: the chart line, bars (3 to 1 on `card` and `card-2`) |
| `accent-text` | `#c2185b` | `#ff8fc0` | links, the selected menu item, focus ring |
| `accent-soft` | `#ffe6f1` | `#3a1d2b` | selected menu item, selected row |
| `on-fill` | `#121016` | `#121016` | anything drawn on `accent` |

### Kinds and states

| Token | Value | Kind of thing | Token | Value | State |
|---|---|---|---|---|---|
| `lilac` | `#978eff` | ETH and staked ETH | `green` | `#17b26a` | up, safe |
| `blue` | `#3b8cff` | dollars | `amber` | `#e5a000` | watch, nearly full, stale |
| `orange` | `#f7931a` | BTC | `red` | `#f0452c` | down, close, full |
| `teal` | `#12b5a5` | euro, gold | | | |
| `ink-2` | (neutral) | anything else | | | |

- `red` is a vermilion, kept well away from the pink accent: a full bar must never look like an
  ordinary one.
- One meaning per colour. A kind colour is never used for a state, and a state colour never for a
  kind. Actions (In, Out, Borrowed, Paid back), roles (Collateral, Borrow) and chains are neutral:
  a grey pill with an arrow or a word, a chain by its logo.
- A rate that moved is grey, in points: cheaper is good for a borrower and bad for a depositor.
- Colour never works alone: a pill has its word, a bar has its number, a change has its arrow.
- On a soft ground a colour is mixed, not picked: ground = 15% of the colour over the surface, text
  = the colour and `ink` in equal parts. That passes 4.5 to 1 on `bg`, `card` and `accent-soft` in
  both themes without a second set of values.
- A fill that carries a value (a bar segment, a legend swatch) must reach 3 to 1 on `card`: in day
  the colour is mixed with a quarter of `ink`; in night it is used as it is.
- A stacked bar of chains is `accent-strong` in four strengths (100, 70, 45, 25%), biggest first;
  each row under the bar repeats its swatch beside the chain's logo.

### Distance from liquidation

The one scale for how safe a loan is, everywhere. Distance = 1 − 1 / health factor: how far the
collateral can fall before the wallet can be liquidated.

| Distance | Health factor | Label | Colour |
|---|---|---|---|
| over 33% | 1.50 and above | Safe | `green` |
| 9% to 33% | 1.10 to 1.49 | Watch | `amber` |
| under 9% | 1.00 to 1.09 | Close | `red` |
| none | below 1.00 | Can be liquidated now | `red`, solid |

The thresholds are the placeholders from [dashboard-data.md](dashboard-data.md); change them there
and here together.

## Typography

Three families, all open source (SIL OFL), self-hosted in the app:

- **Onest** for titles, big numbers and the wordmark (weights 600 and 700). Chosen 2026-11-19 over
  Bricolage Grotesque, Outfit, Sora, Geologica and Figtree.
- **Figtree** for everything else.
- **JetBrains Mono** for what a reader may copy: addresses, hashes.

Figures in tables, rows and pills use tabular numerals. Titles and big numbers set in Onest do not:
its tabular 1 leaves a gap ("$1 .31B").

| Style | Family | Size / line | Weight | Use |
|---|---|---|---|---|
| `display` | title | 40 / 40 | 700 | the value above a chart |
| `title` | title | 32 / 36 | 700 | page title: a token, pool, market |
| `stat` | title | 24 / 24 | 700 | a number in the stat strip |
| `heading` | title | 18 / 22 | 700 | card title |
| `body` | text | 15 / 22 | 400 | text, table cells, rows |
| `strong` | text | 15 / 22 | 600 | names, amounts in a row |
| `small` | text | 14 / 20 | 400 | notes under a card |
| `caption` | text | 13 / 18 | 400 | labels, column headers, the reading stamp |
| `pill` | text | 12 / 12 | 600 | pills, chart ticks (500) |
| `mono` | mono | 13 / 20 | 500 | addresses, hashes |

Title sizes carry −0.02em letter-spacing. Nothing is set below 12px. Sentence case everywhere; the
only capitals are the group names in search results.

## Shape and space

| Token | Value | Use |
|---|---|---|
| `radius-card` | 20px | cards, the stat strip |
| `radius-row` | 12px | a selected row, a tooltip |
| `radius-pill` | 99px | pills, menu items, switches, the search box, bars |

Spacing scale: 4, 8, 12, 16, 20, 24, 32, 48, 64. Cards have 18px padding. The page gutter is 16px,
24px above 720px. Up to 560px wide (a phone) everything is one step tighter: 12px gutter, 14px card
padding, 16px card corners, and the type scale one size down (body 14px, stat 20px, display 32px); content is at most 1040px wide. Depth is a lighter surface and a 1px
`line`. No shadows. A card never sits inside another card.

## Components, the rules that matter

- **Header.** The logo, three menu items (Now, Rates, Close calls), the search box and, at its
  right, one icon button that switches day and night. Nothing else.
- **Search.** One box, opened with `/`. It finds tokens, pools, markets and wallets, grouped in that
  order; before a word is typed it lists the biggest of each. A pasted address opens the wallet.
- **Everything is a link.** A token, pool, market or address shown anywhere links to its page.
  A name is `ink` at weight 600 and underlines in `accent-strong` on hover; an address is `mono` in
  `accent-text`; a way onward ("All rates", "Show all 16 assets") is an underlined `accent-text` link.
- **Breadcrumb and title.** A crumb names the menu page above (Rates, Close calls), then the page.
  Under it: the logo at 44px for a token, the title, the pills that describe it, and the protocol's
  word once ("Pool (Hub)", "Market (Spoke)"). A wallet shows its address with Copy and an explorer
  link, then the gauge.
- **Tooltip.** `ink` ground, `bg` text, `radius-row`, on hover and on focus. It is where the
  protocol's word lives ("The protocol calls this the collateral factor").
- **Show blocks.** Off by default. On, each figure carries "block 26,445,893 · 12s ago" under it in
  12px `ink-2`.
- **Footer.** The not-affiliated line and the Show blocks switch. On every page.
- **Stat strip.** The first thing on a page: three to five figures, each a label, a number and one
  line under it that explains or compares it. It wraps to two columns on a phone, with no empty
  cell.
- **Pills** state a fact: a change, a kind, a chain, a state, an action, a role. One shape, 12px
  text, the colour rule above. A pill is not a button and not a caption: the line under a stat is
  plain `caption` text.
- **Token and chain logos.** 24px in a row, 18px for a chain, 44px in a page title, always with the
  symbol or name beside them, on a 1px `line` ring so a white logo shows on a white card. A logo is
  looked up by contract, never by symbol: `apps/web/public/icons/tokens.json` maps
  `<chain id>:<token address>` to a file in `icons/tokens/` and records where it came from. Chains
  are `icons/chains/<chain id>.svg`: the four v4 chains and the 18 others Aave v3 runs on. Sources, in order: the icon set of Aave's open-source interface
  (vector, most tokens and all four chains), DefiLlama's icons by contract, CoinGecko's token list
  by contract (saved as PNG). All 48 token contracts listed on 2026-11-19 have a file. A contract with no entry
  gets a plain circle in its kind colour with the symbol's first letter; add its logo and its line
  in `tokens.json` in the same change that lists the token. The AAVE and GHO token logos appear
  only as those tokens' logos, at token size, beside their symbol.
- **Tables.** Names left, numbers right, at most six columns, the biggest first. A column header
  sorts the table when pressed; an arrow shows on the sorted column. A long table shows its first
  rows and a "Show more" button; a long list beside a chart scrolls inside its card instead, with
  its head kept in view. A wide table
  scrolls inside its card. A row that has nothing to say in a column says why in words ("Collateral
  only") instead of showing zeros.
- **Lent out and limit used.** Two different things, never both called "how full". *Lent out* is
  borrowed ÷ deposited for an asset in a pool. *Limit used* is what a market has borrowed ÷ its
  limit. Both are a percentage and a thin bar in `accent-strong`. From 90% the bar is `amber` and a
  "Nearly full" pill sits beside the number; at 100% the bar stays, in `red`, with a "Full" pill.
- **Rows of activity.** A grey verb pill with its arrow (↓ In, ↑ Out, ↗ Borrowed, ↩ Paid back;
  Liquidated is the one `red` verb), the USD amount, the token, and the age on the right.
- **Every region that loads data has four states:** loading (grey shapes of the content, no
  spinner), empty, error, full. While it loads, a region, a chart and the page itself hold about
  the room the content will take, so nothing on screen jumps when the data lands.
- **Tables follow their card, not the screen.** In a card up to 860px wide a table drops "on
  Ethereum" after a name, the bar beside a percentage and the long form of a sentence. In a card up
  to 420px wide (a phone, or half a tablet) it becomes a list: the name and its two most useful
  numbers per row, the rest on the item's own page. No table is ever wider than its card.
- **Phones.** A long name wraps so the numbers always fit; a number never wraps. Nothing
  scrolls sideways except a chart being read and a switch with more options than fit. The header is
  two rows (logo, search and theme; then the menu) and scrolls away with the page. An odd last stat
  runs across the strip as one line. A liquidation's time sits on a line under it. Controls are 36px
  high on a phone, 40px on other touch screens, 32px with a mouse.
- **Buttons.** One quiet style: `card-2` ground, `ink` text, pill shape, 32px high (Copy, Try
  again, Show all). A switch between views of one thing (a chart's range, the theme) is a row of
  them on a `card-2` track; the chosen one is `ink` with `bg` text. No primary button: the site is
  read-only.
- **Focus:** a 2px `accent-text` ring, 2px offset, on every interactive element, the chart
  included. Changing a chart's range or metric keeps focus on the control.
- **Icons:** authored SVG, 16px, 1.5px stroke, round ends, the colour of the text beside them:
  search, copy, copied, open in explorer, stale or error, show blocks, day, night, crumb. Never an
  emoji. The arrows in pills (▲ ▼ ↓ ↑ ↗ ↩) are text.
- **Search states.** While typing: the selected row has a ground and a ring. Nothing found, not an
  address, and a wallet with nothing in Aave v4 each say so in one plain sentence.

### Charts

Minimal and exact. Modelled on the rate chart in the Aave app and the price chart on Uniswap.

- Above the chart: the current value in `display`, then its change over the range as a pill.
- One series: a 1.75px `accent-strong` line, a fill under it from 28% `accent` to nothing, a dot
  with a soft halo on the last complete reading.
- A dotted `line` grid. Ticks sit on the right, at round values that bracket the data as tightly as
  round steps allow, all in one format, without zeros they all share (`$0.5B`, `$1.0B`; `$250M`, `$500M`; `5%`, `10%`).
  Five on a desktop, three on a phone. The plot gives the ticks only the room the longest one needs.
- Time labels sit on real boundaries: the first of each month over a long range, midnights for
  days, whole hours inside a day (`18:00`). Months are three letters.
- Axis text is 12px on every screen; it does not shrink with the chart.
- An average is a dashed `ink-2` line with an "Avg" pill, only where an average means something (a
  rate, not a total).
- Controls are pill switches: the range on the left under the chart (1D, 1W, 1M, All), the metric
  on the right above it. The note beside the range says how many readings are drawn and how far
  apart.
- Pointing at the chart, dragging a finger along it, or the arrow keys moves a hairline and the
  dot; the value above becomes the reading at that time, with its date (and the hour only when the
  readings are hourly). Leaving puts it back. A screen reader hears each reading.
- The change above the chart and the same change in a stat strip are one number, computed once.
- No second axis, no 3D, no gradient line, no decoration. A change too large to mean anything (from
  near zero) is written in words: "up from $3.6M on 31 Mar".
- Parts of a whole are one stacked bar with a legend that names and numbers every part, never a
  pie.
- The wallet gauge is a bar from "liquidation" to "50% away" in the three state colours, with the
  band names (Close, Watch, Safe) and the 9% and 33% marks printed on it, scale labels placed by
  their value, and a marker labelled with the distance ("21.4% away"). A wallet further than 50%
  away sits at the right end with its number. It is the first thing on a wallet page.

## Motion

| Token | Duration | Use |
|---|---|---|
| `dur-fast` | 120ms | hover, focus, press, switch |
| `dur-flash` | 600ms | a value that changed over the live stream flashes `accent-soft` once |
| `dur-ping` | 2.4s | the live dot on the mark |

Motion reports a change and then stops. Nothing animates on load. With `prefers-reduced-motion`,
values change with no flash and the live dot is still.

## Themes

Day and night are both designed. The site follows the reader's system setting; the icon button in
the header (a moon in day, a sun in night) overrides it and is remembered.

## Variables

Every token in this file is a CSS variable in `apps/web/src/tokens.css`, the one source for the
app: colours (each written once with `light-dark()`; `color-scheme` picks the side and `data-theme`
overrides it), the three font families and the type styles (`--type-title` and so on, used as
`font: var(--type-title)`), radii, the spacing scale (`--s-1` to `--s-9`), `--gutter`, `--content`,
`--target` (32px, 40px on touch, 36px on a phone) and the three durations. A coloured pill or fill sets `--c` to a
kind or state colour and reads `--pill-ground` and `--pill-text`. Components use variables only,
never a raw colour, size or font name. The fonts are self-hosted in `apps/web/public/fonts/`.

## Share images and favicon

- **Favicon:** `apps/web/public/favicon.svg`, the mark alone.
- **Share images** are PNGs, 1200 by 630, in the night theme: the logo, an optional label
  ("Token", "Pool on Arc" with the chain's logo), the title with the token's logo, one line of
  text, and the not-affiliated line. The Worker draws them on request (`apps/worker/src/share.ts`
  holds the one template and every sentence); nothing is rendered ahead or committed, so a newly
  listed token, pool or market has its image at once.
- The line of text is one live figure, read from the API when the image is drawn, in the words and
  number formats of this file. An image is kept for an hour. If the API does not answer, the line
  is the page's fixed sentence and the image is kept for a minute.

| Image | Title | Live line |
|---|---|---|
| `/og/home.png` | Aave, live. | Aave v4 holds $1.31B, and $518.0M of it is out on loan. |
| `/og/rates.png` | Rates | USDC earns 2.24% and costs 3.12% to borrow on Arc. (the biggest asset that is lent out) |
| `/og/close-calls.png` | Close calls | 305 loans are close to liquidation, and 12 can be liquidated now. |
| `/og/token/<symbol>.png` | the symbol | Earns up to 4.70% (7 days). Borrow from 3.12%. (best and cheapest across pools; "$6.0M deposited across Aave v4." for a token nobody borrows) |
| `/og/pool/<chain id>/<address>.png` | the pool's name | $534.0M deposited, $248.3M borrowed. |
| `/og/market/<chain id>/<address>.png` | the market's name, or `0x94e7…c485` | $429.6M deposited, $111.0M borrowed. |
| `/og/wallet/<address>.png` | `0x1b36…1cd6` | 21.8% from liquidation (its closest loan on any chain) · Can be liquidated now · No loan · Nothing in Aave v4 |

- An image's path is its page's path in lowercase, under `/og/`, ending `.png`. A token, pool or
  market that does not exist answers 404.
- **Link previews.** The Worker also writes each page's title ("USDC on Aave · aave.watch", "Core
  pool on Ethereum · aave.watch"), one-sentence description, address and image into the head of
  the page it serves, so a shared link shows its own page. The wording is in `share.ts` too.
- A logo used in a share image must be SVG or PNG; the renderer cannot read WebP.
- To look at the images before a deploy: `node scripts/og-samples.mts <out dir> /og/home.png ...`
  in `apps/worker`.
