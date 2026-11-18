# Branding

The visual system for aave.watch. Every rule here is shown working in
[visual-identity.html](visual-identity.html): open it in a browser to see each token and component
in every state and in both themes. A value that is not in this file does not go in the UI; if the
two files disagree, fix them in the same change.

## Identity

aave.watch is an instrument: a risk desk for Aave v4. It should feel native to the Aave ecosystem at
first glance and read as its own product at second glance.

- **Shared with the ecosystem:** the colour palette and the Inter typeface.
- **Our own:** dark-first theme, dense table-first layout, small radii, the reading stamp (block and
  age under every state number), our wordmark.
- **Never used:** the Aave logo, the ghost mark, the GHO mark, or Aave's proprietary typefaces
  (Aave Repro, Aave Repro Mono, Aave Aguzzo, FT Regola Neue).

Every page footer carries: *"aave.watch is an independent project. Not affiliated with Aave Labs or
the Aave DAO."*

### Wordmark

`aave.watch`, lowercase, Inter 600, letter-spacing −0.02em, minimum 16px. The dot is in the accent
colour; the letters are the text colour of the theme. No symbol next to it. Favicon: the accent dot
on the page background.

### Voice

Plain and exact. Numbers before adjectives, and the adjective is usually cut.

| We write | We do not write |
|---|---|
| HF 1.03 at block 21,480,112 | Critical position! |
| Draw cap 93.0% used: Main Spoke, USDC | Cap almost full |
| No positions below HF 1.1 in Gold Spoke. | Nothing to see here |
| Could not load positions. The last good reading is block 21,480,098. Retry | Oops! Something went wrong |
| Export CSV | Submit |

- Protocol terms keep the protocol's spelling: Hub, Spoke, reserve, drawn, premium, deficit.
- A state number is never shown without its block.
- Errors say what failed, what the reader is looking at instead, and the one thing to do.
- Empty states say what is absent and where.
- No exclamation marks, no emoji, no "please".
- Sample data is always labelled as sample data.

Number formats: USD `$16.77M`, `$18.4K`; token amounts `41,200,000`; percent `93.0%`; health factor
`1.03`; block `21,480,112`; age `4s ago`, `3h 12m ago`; address `0x94e7…c485`.

## Colour

Palette values come from the public CSS custom properties of aave.com, read on 2026-11-18. The
darker "text" variants, the tinted near-white and near-black, and the strong border are ours. Every
text pair below meets WCAG AA (4.5 to 1) on `bg`, `surface` and `surface-2` in its theme.

### Neutrals

| Token | Dark (default) | Light | Use |
|---|---|---|---|
| `bg` | `#110f0f` | `#f7f6f6` | page |
| `surface` | `#211d1d` | `#fdfcfb` | panels, tables |
| `surface-2` | `#2b2727` | `#f0efee` | hover row, control fill |
| `surface-3` | `#363131` | `#e6e5e4` | control hover, toast, tooltip |
| `pressed` | `#423c3c` | `#d9d8d7` | pressed control |
| `border-subtle` | `#2f2b2b` | `#e9e8e7` | row lines, chart grid |
| `border` | `#444444` | `#dddcdb` | panel edges, dividers |
| `border-strong` | `#706c6c` | `#8f8e8e` | control edges (3 to 1 against its ground) |
| `text` | `#f5f3f3` | `#211d1d` | text, figures |
| `text-2` | `#b8b5b5` | `#636161` | labels, secondary text |
| `text-3` | `#949292` | `#696767` | captions, axis ticks |

The neutrals are warm (a trace of red). No pure black or pure white anywhere. Panels are always
lighter than the page in both themes.

### Accent

| Token | Dark | Light | Use |
|---|---|---|---|
| `accent` | `#978eff` | `#978eff` | fills: primary button, switch on, selected tab line, key chart series |
| `accent-hover` | `#a9a2ff` | `#8a80f5` | primary button hover |
| `accent-pressed` | `#8279e6` | `#7a70e6` | primary button pressed |
| `accent-text` | `#978eff` | `#5446cf` | links, coloured text |
| `accent-soft` | `#2d2a45` | `#e5e5ff` | selected row, pressed chip, value flash |
| `focus` | `#978eff` | `#5446cf` | focus ring |
| `on-fill` | `#110f0f` | `#110f0f` | text on any colour fill. Light text on the accent fails (2.8 to 1) |

One accent. It marks what is interactive or selected, nothing else.

### Status

| Token | Fill (both themes) | Text, dark | Text, light | Meaning |
|---|---|---|---|---|
| `positive` | `#01d062` | `#01d062` | `#00733a` | healthy |
| `warning` | `#ffc42c` | `#ffc42c` | `#805a00` | watch, stale reading |
| `danger` | `#f24900` | `#ff6a2b` | `#bd3900` | at risk, liquidatable, deficit, halted |
| `info` | `#5d7eaa` | `#7f9fca` | `#1c4886` | neutral notice. Not a status; it is also chart series 2 |

`danger-2` (`#ff9a6b`) is the At risk segment in charts, so it reads apart from the solid `danger`
of Liquidatable.

Status colours are reserved for status. They are never decoration, never a chart category, and
never the only signal: a label or an icon always goes with them.

### Risk scale

The one scale for health factor, everywhere:

| Health factor | Label | Colour | Badge |
|---|---|---|---|
| 1.50 and above | Healthy | `positive` | outline |
| 1.10 to 1.49 | Watch | `warning` | outline |
| 1.00 to 1.09 | At risk | `danger` | outline |
| Below 1.00 | Liquidatable | `danger` | solid fill, `on-fill` text |

The thresholds are the placeholders from [dashboard-data.md](dashboard-data.md); change them there
and here together.

### Charts

Series, in order: `#978eff`, `#5d7eaa`, `#d0ceff` (dark) or `#4c1f5e` (light), `#c18fff`, `#94799e`,
`#8f8e8e`. More than six: group the tail as "Other". The three Hubs keep the first three across the
app: Core, Plus, Prime. Label series on the chart; a legend names and numbers every segment. Cap
usage bars are `accent` and switch to `danger` above 90%. Not yet tested on a real multi-series
chart.

## Typography

Two families, both open source (SIL OFL), self-hosted in the app:

- **Inter** for everything.
- **JetBrains Mono** for what a reader may copy: addresses, hashes, event names, token names in
  rules.

Figures always use tabular numerals. Weights 400, 500 and 600. No italics.

| Style | Size / line | Weight | Tracking | Use |
|---|---|---|---|---|
| `display` | 32 / 38 | 600 | −0.03em | a headline figure |
| `h1` | 24 / 30 | 600 | −0.02em | page title |
| `h2` | 18 / 24 | 600 | −0.01em | panel title |
| `prose` | 16 / 26 | 400 | 0 | explanatory text, max 68 characters wide |
| `body` | 14 / 20 | 400 | 0 | UI text, table cells |
| `label` | 12 / 16 | 500 | 0.01em | column headers, field labels, badges |
| `caption` | 12 / 16 | 400 | 0 | reading stamps, help text, axis ticks |
| `mono` | 13 / 20 | 400 | 0 | addresses, hashes |

Nothing is set below 12px. Column headers and labels are sentence case, never capitals. There is no
small label above a heading.

## Shape and space

| Token | Value | Use |
|---|---|---|
| `radius-sm` | 6px | controls, badges |
| `radius-md` | 10px | panels, tables, banners |
| `radius-lg` | 16px | dialogs |
| `radius-pill` | 999px | filter chips, switch |

Spacing scale: 4, 8, 12, 16, 24, 32, 48. Every padding and gap is on it. Controls are 36px high,
32px compact. Table rows are 36px, 32px compact. Panels have 16px padding.

Depth is a lighter surface and a 1px border. No shadows, except one soft shadow on panels in the
light theme. A panel never sits inside another panel.

## Components, the rules that matter

- **Tables are the main component.** Names left, numbers right, units in the header, sortable
  columns, a reading stamp in the foot. Wide tables scroll inside their panel.
- **Every region that loads data has four states:** loading (skeleton rows, no spinner), empty,
  error, full.
- **Reading stamp:** "Read at block 21,480,112, 4s ago" in `caption`, under every table, figure and
  chart. A reading older than its refresh interval is marked "Stale" with an icon and `warning`.
- **Buttons:** primary (accent fill, at most one per view), secondary, quiet. No destructive button:
  the product is read-only. Loading swaps the label for what is happening.
- **Badges state a fact; chips are filters you press.** Neither uses a coloured dot. Heights: badge
  20px, chip 28px, tab 40px.
- **Sort:** the arrow shows only on the sorted column, and on hover or focus elsewhere.
- **Forms:** every control has a visible label. Errors sit under the field with an icon. Filters
  apply as they change.
- **Addresses:** `mono`, shortened, full value on hover and focus, with a copy button.
- **Feedback:** banners for the state of the data, a toast to confirm a small action, a dialog only
  when a choice is needed.
- **Icons:** authored SVG, 16px, 1.5px stroke, one style. Never an emoji.
- **Focus:** 2px `focus` ring, 2px offset, on every interactive element.

## Motion

| Token | Duration | Use |
|---|---|---|
| `dur-fast` | 120ms | hover, focus, press, switch |
| `dur-panel` | 200ms | toast in and out |
| `dur-flash` | 600ms | a cell whose value changed over the live stream flashes `accent-soft` once |

Motion reports a change of state and then stops. Nothing loops and nothing animates on load. With
`prefers-reduced-motion`, values change with no flash.

## Themes

Dark is the default for every reader. Light is a choice the reader makes, and the choice is
remembered. Tokens are CSS custom properties on `:root`, each written once with `light-dark()`;
`color-scheme` on `:root` picks the side, and `data-theme="light"` switches it. Components use
tokens only, never a raw colour or font name.
