// Number and text formats from docs/branding.md ("Voice").

export const DASH = '–'

/** A price: every digit matters, so no K or M. `$3,412.50`, and four decimals below one dollar. */
export function price(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return DASH
  const digits = Math.abs(n) < 1 ? 4 : 2
  return `$${n.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits })}`
}

/** One figure, one format on every page: $1.31B, $137.1M, $788K, $2.54. */
export function usd(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return DASH
  const a = Math.abs(n)
  // the steps sit where rounding would otherwise print "$1000K" or "$1000.0M"
  if (a >= 999.95e6) return `$${(n / 1e9).toFixed(2)}B`
  if (a >= 999_500) return `$${(n / 1e6).toFixed(1)}M`
  if (a >= 1e3) return `$${(n / 1e3).toFixed(0)}K`
  return `$${n.toFixed(2)}`
}

/** An exact dollar amount, used only for a wallet's own holdings. */
export function usdExact(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return DASH
  const digits = n < 1000 ? 2 : 0
  return `$${n.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits })}`
}

/** Raw units (a string, wider than a JS number) to a JS number, for ratios only. */
export function toNumber(raw: string | null | undefined, decimals: number): number {
  if (raw == null || raw === '') return NaN
  const v = BigInt(raw)
  const base = 10n ** BigInt(decimals)
  return Number(v / base) + Number(v % base) / Number(base)
}

/** A rate or a share, from a fraction. Too small to round is said in words. */
export function pct(fraction: number | null | undefined, digits = 2): string {
  if (fraction == null || !Number.isFinite(fraction)) return DASH
  const v = fraction * 100
  return v > 0 && v < 0.5 / 10 ** digits ? `under ${(1 / 10 ** digits).toFixed(digits)}%` : `${v.toFixed(digits)}%`
}

export const block = (n: number | string | null | undefined) => (n == null ? DASH : Number(n).toLocaleString('en-US'))

export const short = (a: string) => (a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a)

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
/** "YYYY-MM-DD hh:mm:ss" (UTC) to milliseconds. */
export const ms = (time: string) => Date.parse(`${time.replace(' ', 'T')}Z`)
/** A UTC time as "14 Nov". */
export const day = (t: number) => { const d = new Date(t); return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}` }
const two = (n: number) => String(n).padStart(2, '0')
/** A UTC time as "18:00". */
export const hour = (t: number) => { const d = new Date(t); return `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}` }
/** A UTC time as "19 Nov 2026, 01:50 UTC". */
export const stamp = (t: number) => `${day(t)} ${new Date(t).getUTCFullYear()}, ${hour(t)} UTC`

/** "4s ago", "10m ago", "1h 35m ago", then the date. */
export function age(time: string | null | undefined, now = Date.now()): string {
  if (!time) return DASH
  const t = ms(time), s = Math.max(0, Math.floor((now - t) / 1000))
  if (Number.isNaN(s)) return DASH
  if (s < 60) return `${s}s ago`
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m ago`
  return day(t)
}

/** Distance from liquidation: how far the collateral can fall, as a fraction. `null` when there is no loan. */
export const distance = (hf: number | null | undefined) => (hf == null ? null : Math.max(0, 1 - 1 / hf))

export type Band = 'now' | 'close' | 'watch' | 'safe'
/** The one safety scale (docs/branding.md). Thresholds are placeholders set in docs/dashboard-data.md. */
export const band = (hf: number | null | undefined): Band => (hf == null || hf >= 1.5 ? 'safe' : hf >= 1.1 ? 'watch' : hf >= 1 ? 'close' : 'now')
export const BAND_LABEL: Record<Band, string> = { now: 'Can be liquidated now', close: 'Close', watch: 'Watch', safe: 'Safe' }
export const BAND_CLASS: Record<Band, string> = { now: 'pill--solid', close: 'c-red', watch: 'c-amber', safe: 'c-green' }

if (import.meta.env.DEV) {
  const t = Date.parse('2026-01-01T03:12:04Z')
  console.assert(usd(1_310_000_000) === '$1.31B' && usd(137_090_000) === '$137.1M' && usd(787_931) === '$788K' && usd(999_700) === '$1.0M' && usd(999_960_000) === '$1.00B' && usd(2.536) === '$2.54' && usd(null) === DASH, 'usd')
  console.assert(usdExact(11221532.4) === '$11,221,532' && usdExact(197.018) === '$197.02', 'usdExact')
  console.assert(pct(0.0496) === '4.96%' && pct(0.68, 0) === '68%' && pct(0.000002) === 'under 0.01%' && pct(0) === '0.00%', 'pct')
  console.assert(age('2026-01-01 03:12:00', t) === '4s ago' && age('2026-01-01 00:00:04', t) === '3h 12m ago' && age('2025-11-14 00:00:00', t) === '14 Nov', 'age')
  console.assert(stamp(Date.parse('2026-11-19T01:50:00Z')) === '19 Nov 2026, 01:50 UTC', 'stamp')
  console.assert(short('0x94e7A5dCbE816e498b89aB752661904E2F56c485') === '0x94e7…c485' && block('21480112') === '21,480,112', 'short')
  console.assert(Math.abs(distance(1.273)! - 0.2145) < 0.0005 && distance(null) === null && distance(0.8) === 0, 'distance')
  console.assert(band(0.98) === 'now' && band(1.03) === 'close' && band(1.18) === 'watch' && band(1.62) === 'safe' && band(null) === 'safe', 'band')
}
