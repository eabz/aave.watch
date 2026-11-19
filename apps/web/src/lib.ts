import manifest from '../public/icons/tokens.json'
import type { Row } from './api.ts'
import { distance, pct } from './format.ts'

export const CHAIN_NAMES: Record<number, string> = { 1: 'Ethereum', 5042: 'Arc', 43114: 'Avalanche', 8453: 'Base' }
export const CHAINS = Object.keys(CHAIN_NAMES).map(Number)
export const chainName = (id: number | string) => CHAIN_NAMES[Number(id)] ?? `Chain ${id}`
// shortcut: explorers are listed here, not read from /v1/chains; move there when a fifth chain is added.
const EXPLORERS: Record<number, string> = { 1: 'https://etherscan.io', 8453: 'https://basescan.org', 43114: 'https://snowtrace.io' }
export const explorer = (chain: number | string, address: string) => (EXPLORERS[Number(chain)] ? `${EXPLORERS[Number(chain)]}/address/${address}` : null)

export type Kind = 'dollar' | 'eth' | 'btc' | 'money' | 'other'
/** The kind of asset a symbol is, for its colour (docs/branding.md). Read from the symbol: there is no list on chain. */
export function kind(symbol: string): Kind {
  const s = symbol.toLowerCase()
  if (s.includes('eur') || s.includes('xau') || s.includes('paxg')) return 'money'
  if (s.includes('usd') || s.includes('gho') || s.includes('dai')) return 'dollar'
  if (s.includes('btc')) return 'btc'
  if (s.includes('eth')) return 'eth'
  return 'other'
}
export const KIND_LABEL: Record<Kind, string> = { dollar: 'Dollar', eth: 'ETH', btc: 'BTC', money: 'Euro, gold', other: 'Other' }
export const KIND_CLASS: Record<Kind, string> = { dollar: 'c-blue', eth: 'c-lilac', btc: 'c-orange', money: 'c-teal', other: '' }
export const KINDS: Kind[] = ['eth', 'dollar', 'btc', 'money', 'other']

const files = manifest as Record<string, { symbol: string; file: string }>
const bySymbol: Record<string, string> = {}
for (const [key, v] of Object.entries(files)) bySymbol[`${key.split(':')[0]}:${v.symbol.toLowerCase()}`] ??= v.file
/** The logo file of a token: by contract, or by symbol on that chain when the row has no token address. */
export const logo = (chain: number | string, token: string | undefined, symbol: string) =>
  files[`${chain}:${(token ?? '').toLowerCase()}`]?.file ?? bySymbol[`${chain}:${symbol.toLowerCase()}`] ?? null

/** Hub-asset rows carry their keys as `s.chain_id`, `s.hub`, `s.asset_id`. */
export const hubKey = (r: Row) => ({ chain: Number(r['s.chain_id'] ?? r.chain_id), hub: String(r['s.hub'] ?? r.hub), asset: Number(r['s.asset_id'] ?? r.asset_id) })
export const num = (v: unknown) => Number(v ?? 0) || 0
/** USDT and USDt are one page. */
export const tokenPath = (symbol: string) => `/token/${encodeURIComponent(symbol.toLowerCase())}`
/** The moves a reader cares about, and the grey verb each one gets. */
export const VERBS: Record<string, string> = { Supply: '↓ In', Withdraw: '↑ Out', Borrow: '↗ Borrowed', Repay: '↩ Paid back', LiquidationCall: 'Liquidated' }
export const MOVES = Object.keys(VERBS)
/** A loan's distance from liquidation, as text. */
export const away = (hf: number | null | undefined) => { const d = distance(hf); return d == null ? 'no loan' : d === 0 ? 'none' : pct(d, 1) }
export const sum = (rows: Row[], key: string) => rows.reduce((a, r) => a + num(r[key]), 0)

if (import.meta.env.DEV) {
  console.assert(kind('USDC') === 'dollar' && kind('weETH') === 'eth' && kind('cbBTC') === 'btc' && kind('EURC') === 'money' && kind('PT-sUSDE-7MAY2026') === 'dollar' && kind('LINK') === 'other', 'kind')
  console.assert(logo(1, '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48', 'USDC') === 'tokens/usdc.svg' && logo(1, undefined, 'weeth') === 'tokens/weeth.svg' && logo(1, undefined, 'nope') === null, 'logo')
}

export const poolPath = (r: Row) => { const k = hubKey(r); return `/pool/${k.chain}/${k.hub}` }
export const marketPath = (r: Row) => `/market/${r.chain_id}/${r.spoke}`
