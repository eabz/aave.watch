import { type ReactNode, useState } from 'react'
import { type Sort, useBlocks, useNow } from './hooks.ts'
import type { Query, Row } from './api.ts'
import { age, band, BAND_CLASS, BAND_LABEL, block, ms, pct, short, stamp, usd } from './format.ts'
import { chainName, explorer, kind, KIND_CLASS, logo, tokenPath, VERBS } from './lib.ts'

// Authored icons: 16px, 1.5px stroke (docs/branding.md).
const ICONS: Record<string, ReactNode> = {
  search: <><circle cx="7" cy="7" r="4.25" /><path d="M10.2 10.2 14 14" /></>,
  copy: <><rect x="5.5" y="5.5" width="8" height="8" rx="2" /><path d="M10.5 5.5v-1a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v4a2 2 0 0 0 2 2h1" /></>,
  check: <path d="m3 8.5 3.2 3.2L13 5" />,
  external: <path d="M9 3h4v4M13 3 7.5 8.5M11 9.5V12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h2.5" />,
  warning: <><path d="M8 2.5 14 13H2L8 2.5Z" /><path d="M8 6.5v3M8 11.2v.1" /></>,
  sun: <><circle cx="8" cy="8" r="2.75" /><path d="M8 1.5v1.5M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" /></>,
  moon: <path d="M13 9.5A5.5 5.5 0 0 1 6.5 3 5.5 5.5 0 1 0 13 9.5Z" />,
  chevron: <path d="m6 3.5 4.5 4.5L6 12.5" />,
  blocks: <><rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1" /><rect x="9" y="2.5" width="4.5" height="4.5" rx="1" /><rect x="2.5" y="9" width="4.5" height="4.5" rx="1" /><rect x="9" y="9" width="4.5" height="4.5" rx="1" /></>,
}
export const Icon = ({ name }: { name: string }) => <svg className="icon" viewBox="0 0 16 16" aria-hidden="true">{ICONS[name]}</svg>

/** The Tick. The dot pulses only while the live stream is connected (live.ts sets `data-live`). */
export const Mark = ({ size = 26 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 48 48" aria-hidden="true">
    <rect width="48" height="48" rx="14" fill="var(--accent)" />
    <polyline points="11,17 18,32 24,21 30,32 37,15" fill="none" stroke="var(--on-fill)" strokeWidth={size < 20 ? 6 : 4.5} strokeLinecap="round" strokeLinejoin="round" />
    <circle className="ping" cx="37" cy="15" r="3" fill="var(--on-fill)" />
  </svg>
)

/** A token: its logo (looked up by contract) and symbol, linking to its page. */
export function Token({ chain, token, symbol, large, bare, plain }: { chain: number | string; token?: string; symbol: string; large?: boolean; bare?: boolean; plain?: boolean }) {
  const file = logo(chain, token, symbol)
  const img = file ? <img src={`/icons/${file}`} alt={bare ? symbol : ''} /> : <i className={KIND_CLASS[kind(symbol)]} aria-hidden="true">{symbol[0]?.toUpperCase()}</i>
  const cls = `tk${large ? ' tk--l' : ''}`
  return plain || bare ? <span className={cls}>{img}{!bare && symbol}</span> : <a className={cls} href={tokenPath(symbol)}>{img}{symbol}</a>
}

/** A chain's logo and name, or a named thing "on" that chain (a pool, a market), linking to it. */
export function Chain({ id, name, href }: { id: number | string; name?: string; href?: string }) {
  const body = <><img src={`/icons/chains/${id}.svg`} alt="" />{name ? <><b>{name}</b><span className="muted">on {chainName(id)}</span></> : chainName(id)}</>
  return href ? <a className="ch" href={href}>{body}</a> : <span className="ch">{body}</span>
}
export const ChainPill = ({ id }: { id: number | string }) => <span className="pill"><img src={`/icons/chains/${id}.svg`} alt="" />{chainName(id)}</span>
export const KindPill = ({ symbol }: { symbol: string }) => { const k = kind(symbol); return k === 'other' ? null : <span className={`pill ${KIND_CLASS[k]}`}>{{ dollar: 'Dollar', eth: 'ETH', btc: 'BTC', money: symbol.toLowerCase().includes('eur') ? 'Euro' : 'Gold', other: '' }[k]}</span> }

/** An amount that changed is green or red, in percent. A rate that changed is grey, in points. */
export function Change({ from, to, rate, word }: { from: number; to: number; rate?: boolean; word?: string }) {
  if (!Number.isFinite(from) || !Number.isFinite(to) || (!rate && !from)) return null
  const d = to - from, text = rate ? `${Math.abs(d * 100).toFixed(2)} pts` : `${Math.abs((d / from) * 100).toFixed(1)}%`
  return <span className={`pill${rate ? '' : d < 0 ? ' c-red' : ' c-green'}`}>{d < 0 ? '▼' : '▲'} {text}{word ? ` ${word}` : ''}</span>
}

/** The safety label of a loan, and its distance from liquidation as text. */
export const BandPill = ({ hf }: { hf: number | null | undefined }) =>
  hf == null ? <span className="pill">No loan</span> : <span className={`pill ${BAND_CLASS[band(hf)]}`}>{BAND_LABEL[band(hf)]}</span>

/** Lent out, or limit used: a percentage and a thin bar that says when it is nearly full. */
export function Used({ value, text, plain }: { value: number | null | undefined; text?: string; plain?: boolean }) {
  if (value == null || !Number.isFinite(value)) return <span className="muted">no limit</span>
  const tone = plain ? '' : value >= 0.995 ? 'c-red' : value >= 0.9 ? 'c-amber' : ''
  return (
    <span className="used">
      {tone && <span className={`pill ${tone}`}>{tone === 'c-red' ? 'Full' : 'Nearly full'}</span>}
      {text ? `${text} · ` : ''}{pct(Math.min(value, 1), value < 0.01 ? 1 : 0)}
      <u><s className={`fill ${tone}`} style={{ width: `${Math.max(1, Math.min(100, value * 100))}%` }} /></u>
    </span>
  )
}

export const Blk = ({ at }: { at?: Row | null }) => {
  const now = useNow()
  return at?.block_number ? <span className="blk">block {block(at.block_number)} · {age(at.block_time, now)}</span> : null
}
export const Stat = ({ label, value, at, children }: { label: string; value: ReactNode; at?: Row | null; children?: ReactNode }) => (
  <div className="stat"><span className="caption">{label}</span><b>{value}</b>{children && <span className="caption pills">{children}</span>}<Blk at={at} /></div>
)
export const Card = ({ title, tools, note, children }: { title?: ReactNode; tools?: ReactNode; note?: ReactNode; children: ReactNode }) => (
  <section className="card">
    {(title || tools) && <div className="head">{title && <h2>{title}</h2>}{tools}</div>}
    {children}
    {note && <span className="caption">{note}</span>}
  </section>
)
/** A column header that sorts its table. The arrow shows on the sorted column. */
export const SortTh = ({ k, sort, toggle, r, children }: { k: string; sort: Sort; toggle: (k: string) => void; r?: boolean; children: ReactNode }) => (
  <th scope="col" className={r ? 'r' : undefined} aria-sort={sort.key === k ? sort.dir : undefined}>
    <button className="sort" type="button" onClick={() => toggle(k)}>{children}{sort.key === k && <span aria-hidden="true">{sort.dir === 'ascending' ? ' ↑' : ' ↓'}</span>}</button>
  </th>
)
export const Crumb = ({ to, label, here }: { to: string; label: string; here: string }) => (
  <nav className="crumb" aria-label="Breadcrumb"><a href={to}>{label}</a><Icon name="chevron" />{here}</nav>
)

/** The four states of anything that loads: grey shapes, an error with one thing to do, an empty sentence, the content. */
export function Region({ q, empty, what, children }: { q: Query<{ data: Row[] }>; empty: string; what: string; children: (rows: Row[]) => ReactNode }) {
  const rows = q.data?.data
  if (!rows && q.loading) return <div className="state" aria-busy="true">{[60, 85, 40].map((w) => <span key={w} className="skel" style={{ width: `${w}%` }} />)}</div>
  if (!rows) return <div className="state" role="alert"><span>Could not load {what}. {q.error ?? 'The API did not answer'}.</span><button className="btn" type="button" onClick={q.refetch}>Try again</button></div>
  return (
    <>
      {q.error && <div className="state" role="status"><span className="pills"><span className="pill c-amber"><Icon name="warning" />Stale</span>Could not refresh {what}. Showing the last good reading.</span><button className="btn" type="button" onClick={q.refetch}>Try again</button></div>}
      {rows.length === 0 ? <span className="muted">{empty}</span> : children(rows)}
    </>
  )
}

export function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => navigator.clipboard?.writeText(value).then(() => { setCopied(true); setTimeout(() => setCopied(false), 2000) }, () => {})
  return <button className="btn" type="button" onClick={copy}><Icon name={copied ? 'check' : 'copy'} />{copied ? 'Copied' : 'Copy'}</button>
}
/** A wallet, as a link to its page. */
export const Wallet = ({ address }: { address: string }) => <a className="mono" href={`/wallet/${address}`} title={address}>{short(address)}</a>
export const Explorer = ({ chain, address }: { chain: number | string; address: string }) => {
  const href = explorer(chain, address)
  return href ? <a className="pills" href={href} target="_blank" rel="noreferrer">Explorer <Icon name="external" /></a> : null
}

/** One move: a grey verb, the amount, the token, and when. */
export function Move({ e, now }: { e: Row; now: number }) {
  return (
    <div className="row">
      <span className="pills">
        <span className={`pill${e.name === 'LiquidationCall' ? ' c-red' : ''}`}>{VERBS[e.name] ?? e.name}</span>
        <b>{usd(e.amount_usd)}</b>
        {e.symbol && <Token chain={e.chain_id} token={e.token} symbol={e.symbol} />}
      </span>
      <span className="muted">{age(e.block_time, now)}</span>
    </div>
  )
}

/** Says so when a page's newest reading is over two hours old. Fresh readings need no line. */
export function Stamp({ rows }: { rows: (Row | null | undefined)[] }) {
  const now = useNow()
  const times = rows.filter((r) => r?.block_time).map((r) => ms(r!.block_time))
  const newest = times.length ? Math.max(...times) : null
  if (newest == null || now - newest < 120 * 60_000) return null
  return <div className="pills caption"><span className="pill c-amber"><Icon name="warning" />Stale</span><span>read {stamp(newest)}</span></div>
}

/** The Show blocks switch, in the footer: puts the block and its age under every figure. */
export function BlocksSwitch() {
  const [blocks, toggle] = useBlocks()
  return <button className="btn" type="button" aria-pressed={blocks} onClick={toggle}><Icon name="blocks" />{blocks ? 'Hide blocks' : 'Show blocks'}</button>
}
