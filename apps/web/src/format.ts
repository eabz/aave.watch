// Number and text formats from docs/branding.md ("Voice").

export const DASH = '–'

/** A price: every digit matters, so no K or M. `$3,412.50`, and four decimals below one dollar. */
export function price(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return DASH
  const digits = Math.abs(n) < 1 ? 4 : 2
  return `$${n.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits })}`
}

export function usd(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return DASH
  const a = Math.abs(n)
  if (a >= 1e9) return `$${(n / 1e9).toFixed(2)}B`
  if (a >= 1e6) return `$${(n / 1e6).toFixed(2)}M`
  if (a >= 1e3) return `$${(n / 1e3).toFixed(1)}K`
  return `$${n.toFixed(2)}`
}

/** Raw token units (a string, wider than a JS number) to whole tokens with separators. */
export function amount(raw: string | null | undefined, decimals: number): string {
  if (raw == null || raw === '') return DASH
  let v: bigint
  try { v = BigInt(raw) } catch { return DASH }
  const neg = v < 0n
  if (neg) v = -v
  const base = 10n ** BigInt(decimals)
  const whole = v / base
  const grouped = whole.toLocaleString('en-US')
  const text = whole >= 1000n ? grouped : `${grouped}.${(v % base).toString().padStart(decimals, '0').slice(0, 2).padEnd(2, '0')}`
  return neg ? `-${text}` : text
}

/** A ray-scaled amount (27 extra decimals), as the Hub reports deficit. */
export const rayAmount = (ray: string | null | undefined, decimals: number) =>
  ray == null || ray === '' ? DASH : amount((BigInt(ray) / 10n ** 27n).toString(), decimals)

/** Raw units to a JS number, for ratios only (never for display of the amount itself). */
export function toNumber(raw: string | null | undefined, decimals: number): number {
  if (raw == null || raw === '') return NaN
  const v = BigInt(raw)
  const base = 10n ** BigInt(decimals)
  return Number(v / base) + Number(v % base) / Number(base)
}

export const pct = (fraction: number | null | undefined, digits = 1) =>
  fraction == null || !Number.isFinite(fraction) ? DASH : `${(fraction * 100).toFixed(digits)}%`

export const bps = (n: number | string | null | undefined, digits = 1) => (n == null ? DASH : pct(Number(n) / 10_000, digits))

export const hf = (n: number | null | undefined) => (n == null ? 'No debt' : n.toFixed(2))

export const block = (n: number | string | null | undefined) => (n == null ? DASH : Number(n).toLocaleString('en-US'))

export const short = (a: string) => (a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a)

/** "YYYY-MM-DD hh:mm:ss" (UTC) to "4s ago", "5m ago", "3h 12m ago", "2d 4h ago". */
export function age(time: string | null | undefined, now = Date.now()): string {
  if (!time) return DASH
  const s = Math.max(0, Math.floor((now - Date.parse(`${time.replace(' ', 'T')}Z`)) / 1000))
  if (Number.isNaN(s)) return DASH
  if (s < 60) return `${s}s ago`
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m ago`
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h ago`
}

export type Band = 'liq' | 'risk' | 'watch' | 'healthy'
/** The one risk scale (docs/branding.md). Thresholds are placeholders set in docs/dashboard-data.md. */
export const band = (h: number | null | undefined): Band => (h == null || h >= 1.5 ? 'healthy' : h >= 1.1 ? 'watch' : h >= 1 ? 'risk' : 'liq')
export const BAND_LABEL: Record<Band, string> = { liq: 'Liquidatable', risk: 'At risk', watch: 'Watch', healthy: 'Healthy' }
export const BAND_TONE: Record<Band, string> = { liq: 'danger', risk: 'danger', watch: 'warning', healthy: 'positive' }

if (import.meta.env.DEV) {
  const t = Date.parse('2026-01-01T03:12:04Z')
  console.assert(usd(16_770_000) === '$16.77M' && usd(18_400) === '$18.4K' && usd(null) === DASH, 'usd')
  console.assert(amount('41200000000000', 6) === '41,200,000' && amount('1500000', 6) === '1.50', 'amount')
  console.assert(rayAmount(`18400000000${'0'.repeat(27)}`, 6) === '18,400', 'rayAmount')
  console.assert(pct(0.93) === '93.0%' && bps(420, 2) === '4.20%' && hf(1.034) === '1.03' && hf(null) === 'No debt', 'pct')
  console.assert(age('2026-01-01 03:12:00', t) === '4s ago' && age('2026-01-01 00:00:04', t) === '3h 12m ago', 'age')
  console.assert(short('0x94e7A5dCbE816e498b89aB752661904E2F56c485') === '0x94e7…c485' && block('21480112') === '21,480,112', 'short')
  console.assert(band(0.98) === 'liq' && band(1.03) === 'risk' && band(1.18) === 'watch' && band(1.62) === 'healthy' && band(null) === 'healthy', 'band')
}
