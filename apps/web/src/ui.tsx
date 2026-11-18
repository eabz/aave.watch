import { type ReactNode, useEffect, useState } from 'react'
import type { Query, Row } from './api.ts'
import { age, band, BAND_LABEL, BAND_TONE, block, hf, short } from './format.ts'
import { chainName, type Sort } from './lib.ts'

export const Icon = ({ name }: { name: string }) => (
  <svg className="icon" aria-hidden="true"><use href={`#i-${name}`} /></svg>
)

/** Re-renders on a timer so "4s ago" keeps counting. */
function useNow(ms = 5000) {
  const [now, setNow] = useState(Date.now)
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), ms); return () => clearInterval(t) }, [ms])
  return now
}

/** The reading stamp: every state number carries its block and age (docs/branding.md). */
export function Stamp({ rows, note }: { rows: Row[] | null | undefined; note?: string }) {
  const now = useNow()
  if (!rows?.length) return <span className="t-caption">{note ?? ''}</span>
  const newest = rows.reduce((a, r) => (Number(r.block_number) > Number(a.block_number) ? r : a))
  const chains = new Set(rows.map((r) => r.chain_id))
  return (
    <span className="t-caption">
      {chains.size > 1 ? `Newest reading: ${chainName(newest.chain_id)} block` : 'Read at block'} {block(newest.block_number)}, {age(newest.block_time, now)}.
      {note ? ` ${note}` : ''}
    </span>
  )
}

/** The four states of a data region: skeleton, error, empty, full. */
export function Region({ q, empty, what, children }: { q: Query<{ data: Row[] }>; empty: string; what: string; children: (rows: Row[]) => ReactNode }) {
  const rows = q.data?.data
  if (!rows && q.loading) {
    return (
      <table className="table" aria-hidden="true">
        <tbody>
          {[60, 45, 70, 50].map((w, i) => (
            <tr key={i}>
              <td><span className="skel" style={{ inlineSize: `${w}%` }} /></td>
              <td><span className="skel" style={{ inlineSize: `${w - 20}%`, marginLeft: 'auto' }} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    )
  }
  if (!rows) {
    return (
      <div className="state state--error" role="alert">
        <Icon name="alert" />
        <p>Could not load {what}. {q.error ?? 'The API did not answer'}. Nothing is shown in its place.</p>
        <button className="btn btn--sm" type="button" onClick={q.refetch}>Retry</button>
      </div>
    )
  }
  return (
    <>
      {q.error && (
        <div className="banner banner--warning" role="status" style={{ margin: 'var(--s-3) var(--s-4) 0' }}>
          <Icon name="alert" />
          <p>Could not refresh {what}: {q.error}. The rows below are the last good reading.</p>
          <button className="btn btn--sm" type="button" onClick={q.refetch}>Retry</button>
        </div>
      )}
      {rows.length === 0 ? <div className="state"><Icon name="empty" /><p>{empty}</p></div> : children(rows)}
    </>
  )
}

export function Address({ value, label, href }: { value: string; label?: string; href?: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => navigator.clipboard?.writeText(value).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1500) }, () => {})
  return (
    <span className="addr">
      <span className="tip">
        {href ? <a className="mono" href={href}>{short(value)}</a> : <span className="mono" tabIndex={0}>{short(value)}</span>}
        <span className="tip__bubble mono" role="tooltip">{value}</span>
      </span>
      <button className="btn btn--icon" type="button" onClick={copy} aria-label={copied ? 'Copied' : `Copy ${label ?? 'address'} ${short(value)}`}>
        <Icon name={copied ? 'check' : 'copy'} />
      </button>
    </span>
  )
}

export const HfFigure = ({ value }: { value: number | null }) => (
  <span style={value == null ? undefined : { color: `var(--${BAND_TONE[band(value)]}-text)` }}>{hf(value)}</span>
)
export const HfBadge = ({ value }: { value: number | null }) =>
  value == null ? <span className="badge">No debt</span> : <span className={`badge badge--${band(value)}`}>{BAND_LABEL[band(value)]}</span>

export function SortTh({ label, k, sort, toggle, num }: { label: string; k: string; sort: Sort; toggle: (k: string) => void; num?: boolean }) {
  return (
    <th scope="col" className={num ? 'num' : undefined} aria-sort={sort.key === k ? sort.dir : undefined}>
      <button className="sort" type="button" onClick={() => toggle(k)}>{label} <Icon name="sort" /></button>
    </th>
  )
}

export const Panel = ({ title, tools, foot, children }: { title: string; tools?: ReactNode; foot?: ReactNode; children: ReactNode }) => (
  <section className="panel" aria-label={title}>
    <div className="panel__head"><div className="grow"><h2 className="t-h2">{title}</h2></div>{tools}</div>
    <div className="scroll">{children}</div>
    {foot && <div className="panel__foot">{foot}</div>}
  </section>
)
