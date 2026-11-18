import { type KeyboardEvent, type PointerEvent, useState } from 'react'
import { type Row, useApi } from '../api.ts'
import { pct, usd } from '../format.ts'
import { chainName } from '../lib.ts'
import { Panel, Region, Stamp } from '../ui.tsx'

type Point = { hour: string; supplied: number; borrowed: number }
type Series = { label: string; color: string; values: number[] }
const STEPS = { day: ['Daily, since launch', 24 * 366 * 5], hour: ['Hourly, 7 days', 168] } as const
type Step = keyof typeof STEPS

/** One point per time across the chains in `rows`. A chain not yet launched at a time adds nothing. */
function totals(rows: Row[]): Point[] {
  const by = new Map<string, Point>()
  for (const r of rows) {
    const p = by.get(r.hour) ?? { hour: r.hour, supplied: 0, borrowed: 0 }
    p.supplied += Number(r.supplied_usd); p.borrowed += Number(r.borrowed_usd)
    by.set(r.hour, p)
  }
  return [...by.values()].sort((a, b) => (a.hour < b.hour ? -1 : 1))
}

/** v4 as a fraction of v3 + v4 at every time both have a reading. */
function shares(v4: Point[], v3: Point[]) {
  const old = new Map(v3.map((p) => [p.hour, p]))
  return v4.flatMap((p) => {
    const o = old.get(p.hour)
    return o && o.supplied > 0 ? [{ hour: p.hour, supplied: p.supplied / (p.supplied + o.supplied), borrowed: p.borrowed / (p.borrowed + o.borrowed) }] : []
  })
}

const signed = (n: number) => `${n < 0 ? '−' : '+'}${usd(Math.abs(n))}`
const signedPct = (now: number, then: number) => (then > 0 ? `${now < then ? '−' : '+'}${pct(Math.abs(now / then - 1), 2)}` : '–')
const points = (now: number, then: number) => `${now < then ? '−' : '+'}${(Math.abs(now - then) * 100).toFixed(2)} points`
const when = (hour: string, step: Step) => (step === 'day' && hour.endsWith('00:00:00') ? hour.slice(0, 10) : `${hour.slice(0, 16)} UTC`)

const X0 = 56, X1 = 664, Y0 = 200, Y1 = 20

/** Lines over time. Pointer or arrow keys move a cursor; the key under the chart reads the values at it. */
function Chart({ name, times, series, format, area }: { name: string; times: string[]; series: Series[]; format: (n: number) => string; area?: boolean }) {
  const [cursor, setCursor] = useState<number | null>(null)
  const n = times.length, at = cursor === null ? n - 1 : Math.min(cursor, n - 1)
  const top = Math.max(...series.flatMap((s) => s.values), 1e-9) * 1.08
  const x = (i: number) => X0 + ((X1 - X0) * i) / Math.max(1, n - 1)
  const y = (v: number) => Y0 - ((Y0 - Y1) * v) / top
  const path = (s: Series) => s.values.map((v, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join('')
  const move = (e: PointerEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect()
    const vx = ((e.clientX - box.left) / box.width) * 680
    setCursor(Math.max(0, Math.min(n - 1, Math.round(((vx - X0) / (X1 - X0)) * (n - 1)))))
  }
  const key = (e: KeyboardEvent) => {
    const d = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0
    if (!d) return
    e.preventDefault()
    setCursor(Math.max(0, Math.min(n - 1, at + d)))
  }
  const ends = series.map((s) => `${s.label} from ${format(s.values[0])} to ${format(s.values[n - 1])}`).join('; ')
  return (
    <div className="panel__body">
      <div className="scroll" tabIndex={0} role="group" aria-label={`${name}. Left and right arrows move the reading.`} onKeyDown={key} onBlur={() => setCursor(null)}>
        <svg className="chart chart--wide" viewBox="0 0 680 224" role="img" onPointerMove={move} onPointerLeave={() => setCursor(null)}
          aria-label={`${name}, ${times[0]} to ${times[n - 1]}: ${ends}.`}>
          <path className="grid" d={`M${X0} ${Y0}H${X1}M${X0} ${(Y0 + Y1) / 2}H${X1}M${X0} ${Y1}H${X1}`} />
          <text x={X0 - 8} y={Y0 + 4} textAnchor="end">{format(0)}</text>
          <text x={X0 - 8} y={(Y0 + Y1) / 2 + 4} textAnchor="end">{format(top / 2)}</text>
          <text x={X0 - 8} y={Y1 + 4} textAnchor="end">{format(top)}</text>
          <text x={X0} y="218">{times[0]}</text><text x={X1} y="218" textAnchor="end">{times[n - 1]}</text>
          {area && <path d={`${path(series[0])}L${X1} ${Y0}L${X0} ${Y0}Z`} style={{ fill: 'var(--accent-soft)' }} />}
          {[...series].reverse().map((s) => <path key={s.label} className="line" d={path(s)} style={{ stroke: s.color }} />)}
          <path className="ref" d={`M${x(at).toFixed(1)} ${Y1}V${Y0}`} style={cursor === null ? { display: 'none' } : undefined} />
          {series.map((s) => <circle key={s.label} className="end" cx={x(at)} cy={y(s.values[at])} r="4" style={{ fill: s.color }} />)}
        </svg>
      </div>
      <p className="chips" style={{ marginTop: 'var(--s-3)' }} aria-live="off">
        <span className="t-caption">{times[at]}</span>
        {series.map((s) => <span key={s.label} className="key"><i style={{ '--k': s.color } as React.CSSProperties} />{s.label} {format(s.values[at])}</span>)}
      </p>
    </div>
  )
}

function Steps({ step, set }: { step: Step; set: (s: Step) => void }) {
  return (
    <fieldset className="seg">
      <legend className="visually-hidden">Step</legend>
      {(Object.keys(STEPS) as Step[]).map((s) => (
        <span key={s} style={{ display: 'contents' }}>
          <input type="radio" name="step" id={`st-${s}`} checked={step === s} onChange={() => set(s)} />
          <label htmlFor={`st-${s}`}>{STEPS[s][0]}</label>
        </span>
      ))}
    </fieldset>
  )
}

/** The newest reading of each chain, v3 next to v4. */
function ByChain({ v3, v4 }: { v3: Row[]; v4: Row[] }) {
  const chains = new Map<number, { name: string; v3?: Row; v4?: Row }>()
  for (const r of v3) chains.set(r.chain_id, { name: r.chain || chainName(r.chain_id), v3: r }) // rows are oldest first: the last one stays
  for (const r of v4) chains.set(r.chain_id, { name: chainName(r.chain_id), ...chains.get(r.chain_id), v4: r })
  const n = (r: Row | undefined, k: string) => Number(r?.[k] ?? 0)
  const rows = [...chains.entries()].sort((a, b) => n(b[1].v3, 'supplied_usd') + n(b[1].v4, 'supplied_usd') - n(a[1].v3, 'supplied_usd') - n(a[1].v4, 'supplied_usd'))
  return (
    <table className="table">
      <thead>
        <tr>
          <th scope="col">Chain</th>
          <th scope="col" className="num">v3 supplied</th><th scope="col" className="num">v4 supplied</th><th scope="col">v4 share of supplied</th>
          <th scope="col" className="num">v3 borrowed</th><th scope="col" className="num">v4 borrowed</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(([id, c]) => {
          const share = c.v4 ? n(c.v4, 'supplied_usd') / (n(c.v4, 'supplied_usd') + n(c.v3, 'supplied_usd')) : null
          return (
            <tr key={id}>
              <td>{c.name}</td>
              <td className="num">{c.v3 ? usd(n(c.v3, 'supplied_usd')) : <span className="muted">No v3 market</span>}</td>
              <td className="num">{c.v4 ? usd(n(c.v4, 'supplied_usd')) : <span className="muted">Not deployed</span>}</td>
              <td>
                {share === null ? <span className="muted">–</span> : (
                  <span style={{ display: 'inline-grid', gridTemplateColumns: '6rem max-content', gap: 'var(--s-2)', alignItems: 'center' }}>
                    <span className="meter" role="img" aria-label={`${pct(share)} of Aave on ${c.name} is in v4`}><i style={{ '--v': share } as React.CSSProperties} /></span>
                    <span>{pct(share)}</span>
                  </span>
                )}
              </td>
              <td className="num">{c.v3 ? usd(n(c.v3, 'borrowed_usd')) : '–'}</td>
              <td className="num">{c.v4 ? usd(n(c.v4, 'borrowed_usd')) : '–'}</td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

export function Adoption({ chain }: { chain: number }) {
  const [step, setStep] = useState<Step>('day')
  const params = { chain, step, hours: STEPS[step][1] }
  const q4 = useApi('/v1/history/totals', params, 300_000)
  const q3 = useApi('/v1/history/v3-totals', params, 300_000)
  const v4 = totals(q4.data?.data ?? []), v3 = totals(q3.data?.data ?? [])
  const share = shares(v4, v3)
  const last = v4[v4.length - 1], prev = v4[v4.length - 2]
  const s1 = share[share.length - 1], s0 = share[0], old = v3[v3.length - 1]
  const unit = step === 'day' ? 'day' : 'hour'
  const where = chain ? chainName(chain) : 'all chains'
  const times = (ps: { hour: string }[]) => ps.map((p) => when(p.hour, step))
  // the newest daily point is the running hour of today, so "last day" compares it with today 00:00
  const figures: [string, string, string][] = [
    ['v4 share of Aave supplied', s1 ? pct(s1.supplied, 2) : '–', s1 ? `${points(s1.supplied, s0.supplied)} since ${when(s0.hour, step)}` : 'No v3 reading yet.'],
    ['Supplied to v4', last ? usd(last.supplied) : '–', last && prev ? `${signed(last.supplied - prev.supplied)} (${signedPct(last.supplied, prev.supplied)}) in the last ${unit}` : ''],
    ['Supplied to v3', old ? usd(old.supplied) : '–', old ? `${signed(old.supplied - v3[0].supplied)} since ${when(v3[0].hour, step)}` : ''],
  ]

  return (
    <div className="page">
      <header><h1 className="t-h1">Adoption</h1><p className="muted">How much of Aave on {where} has moved to v4, and how v4 grows per {unit}.</p></header>
      <section className="panel" aria-label="Adoption figures">
        <div className="stats">
          {figures.map(([label, value, note]) => (
            <div key={label}><span className="t-label">{label}</span><span className="t-display">{value}</span><span className="t-caption">{note}</span></div>
          ))}
        </div>
      </section>
      <Panel title={`v4 share of Aave, ${where}`} tools={<Steps step={step} set={setStep} />}
        foot={<Stamp rows={q3.data?.data} note="Share is v4 / (v3 + v4) in USD. v3 is every market in Aave's address book, Horizon and the Ink whitelabel included." />}>
        <Region q={q3} what="the v3 history" empty={`No v3 readings for ${where} yet.`}>
          {() => (share.length > 1
            ? <Chart name="v4 share of Aave" times={times(share)} format={(v) => pct(v, 2)} area series={[
                { label: 'Share of supplied', color: 'var(--series-1)', values: share.map((p) => p.supplied) },
                { label: 'Share of borrowed', color: 'var(--series-2)', values: share.map((p) => p.borrowed) },
              ]} />
            : <div className="state"><p>v3 and v4 have no reading at the same time on {where} yet.</p></div>)}
        </Region>
      </Panel>
      <Panel title="v3 and v4 by chain" foot={<Stamp rows={q3.data?.data} note="The newest hourly reading of each chain." />}>
        <Region q={q3} what="the v3 totals" empty={`No v3 readings for ${where} yet.`}>
          {(rows) => <ByChain v3={rows} v4={q4.data?.data ?? []} />}
        </Region>
      </Panel>
      <Panel title={`Supplied and borrowed in v4, ${where}`}
        foot={<Stamp rows={q4.data?.data} note="USD at the oracle price of each reading. Daily points are the 00:00 UTC reading; the newest point is the current hour." />}>
        <Region q={q4} what="the supplied and borrowed history" empty={`No hourly readings for ${where} yet.`}>
          {() => (
            <>
              {v4.length > 1 && <Chart name="Supplied and borrowed in v4" times={times(v4)} format={usd} series={[
                { label: 'Supplied', color: 'var(--series-1)', values: v4.map((p) => p.supplied) },
                { label: 'Borrowed', color: 'var(--series-2)', values: v4.map((p) => p.borrowed) },
              ]} />}
              <table className="table">
                <thead>
                  <tr>
                    <th scope="col">{step === 'day' ? 'Day' : 'Hour'}</th>
                    <th scope="col" className="num">Supplied</th><th scope="col" className="num">Change</th><th scope="col" className="num">Change, %</th>
                    <th scope="col" className="num">Borrowed</th><th scope="col" className="num">Change</th><th scope="col" className="num">Change, %</th>
                  </tr>
                </thead>
                <tbody>
                  {v4.map((p, i) => ({ p, b: v4[i - 1] })).reverse().slice(0, 30).map(({ p, b }) => (
                    <tr key={p.hour}>
                      <td>{when(p.hour, step)}</td>
                      <td className="num">{usd(p.supplied)}</td><td className="num">{b ? signed(p.supplied - b.supplied) : '–'}</td><td className="num">{b ? signedPct(p.supplied, b.supplied) : '–'}</td>
                      <td className="num">{usd(p.borrowed)}</td><td className="num">{b ? signed(p.borrowed - b.borrowed) : '–'}</td><td className="num">{b ? signedPct(p.borrowed, b.borrowed) : '–'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </Region>
      </Panel>
    </div>
  )
}

if (import.meta.env.DEV) {
  const t = totals([
    { hour: '2026-01-02 00:00:00', supplied_usd: 5, borrowed_usd: 1 }, { hour: '2026-01-01 00:00:00', supplied_usd: 2, borrowed_usd: 1 },
    { hour: '2026-01-02 00:00:00', supplied_usd: 3, borrowed_usd: 2 },
  ])
  console.assert(t.length === 2 && t[0].supplied === 2 && t[1].supplied === 8 && t[1].borrowed === 3, 'totals')
  const s = shares(t, [{ hour: '2026-01-02 00:00:00', supplied: 24, borrowed: 9 }])
  console.assert(s.length === 1 && s[0].supplied === 0.25 && s[0].borrowed === 0.25, 'shares')
  console.assert(signed(-1500) === '−$1.5K' && signedPct(110, 100) === '+10.00%' && signedPct(1, 0) === '–' && points(0.05, 0.04) === '+1.00 points', 'signed')
}
