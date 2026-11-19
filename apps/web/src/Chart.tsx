import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from 'react'
import { day, hour } from './format.ts'
import { Change } from './ui.tsx'
import type { ReactNode } from 'react'

// The one chart (docs/branding.md, "Charts"): the value and its change above, one thin line with a
// soft fill, a dotted grid, round ticks on the right, time labels on real boundaries. Drawn at real
// pixels and redrawn on resize, so axis text never shrinks.

export type Point = [time: number, value: number] // unix seconds, oldest first
export type Series = { label: string; points: Point[]; fmt: (v: number) => string; tick?: (max: number) => (v: number) => string; rate?: boolean }
export type Range = { label: string; n: number; word: string }

const DAY = 86400

/** Round tick values that bracket lo..hi in about n steps. */
function ticks(lo: number, hi: number, n: number): number[] {
  const raw = (hi - lo) / n || Math.abs(hi) / n || 1
  const mag = 10 ** Math.floor(Math.log10(raw))
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw)!
  const out: number[] = []
  for (let v = Math.floor(lo / step) * step; v < hi + step; v += step) out.push(+v.toFixed(10))
  return out
}

/** Label times on real boundaries: month starts over a long range, midnights for days, whole hours inside two days. */
function marks(t0: number, t1: number, max: number): { t: number; text: string }[] {
  const all: number[] = []
  const unit = t1 - t0 > 2 * DAY ? DAY : 6 * 3600
  if (t1 - t0 > 60 * DAY) {
    const a = new Date(t0 * 1000)
    for (let m = a.getUTCMonth() + 1; ; m++) { const t = Date.UTC(a.getUTCFullYear(), m, 1) / 1000; if (t > t1) break; all.push(t) }
  } else for (let t = Math.ceil(t0 / unit) * unit; t <= t1; t += unit) all.push(t)
  const every = Math.ceil(all.length / max)
  return all.filter((_, i) => (all.length - 1 - i) % every === 0).map((t) => ({ t, text: unit === DAY || t % DAY === 0 ? day(t * 1000) : hour(t * 1000) }))
}

export function Chart({ id, title, series, ranges, avg, hourly, tools }: { id: string; title: string; series: Series[]; ranges: Range[]; avg?: boolean; hourly?: boolean; tools?: ReactNode }) {
  const box = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  const [si, setSi] = useState(0)
  const [ri, setRi] = useState(ranges.length - 1)
  const [at, setAt] = useState<number | null>(null)
  useEffect(() => {
    const el = box.current!
    const ro = new ResizeObserver(() => setWidth(el.clientWidth))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  const s = series[Math.min(si, series.length - 1)]
  const range = ranges[ri]
  const pts = range.n ? s.points.slice(-range.n) : s.points
  const head = (
    <div className="ctl">
      <h2>{title}</h2>
      <span className="pills">
        {tools}
        {series.length > 1 && <span className="seg" role="group" aria-label="What to chart">{series.map((x, i) => <button key={x.label} type="button" aria-pressed={i === si} onClick={() => { setSi(i); setAt(null) }}>{x.label}</button>)}</span>}
      </span>
    </div>
  )
  if (pts.length < 2) return <div className="chart" ref={box}>{head}<span className="muted">Not enough readings to draw a line yet.</span></div>

  const W = Math.max(240, width), small = W < 480, H = small ? 200 : 250
  const X0 = 4, X1 = W - 62, Y0 = 14, Y1 = H - 30
  const vs = pts.map((p) => p[1])
  const tk = ticks(Math.min(...vs), Math.max(...vs), small ? 3 : 4)
  const lo = tk[0], hi = tk[tk.length - 1], t0 = pts[0][0], t1 = pts[pts.length - 1][0]
  const tf = s.tick?.(hi) ?? s.fmt
  const x = (t: number) => X0 + ((t - t0) / (t1 - t0)) * (X1 - X0)
  const y = (v: number) => Y1 - ((v - lo) / (hi - lo)) * (Y1 - Y0)
  const d = pts.map((p, i) => `${i ? 'L' : 'M'}${x(p[0]).toFixed(1)} ${y(p[1]).toFixed(1)}`).join('')
  const mean = vs.reduce((a, b) => a + b, 0) / vs.length
  const first = pts[0], last = pts[pts.length - 1], cur = pts[at ?? pts.length - 1] ?? last
  const huge = !s.rate && first[1] > 0 && Math.abs(last[1] / first[1] - 1) > 9.99
  const when = (t: number) => (hourly ? `${day(t * 1000)}, ${hour(t * 1000)} UTC` : day(t * 1000))

  const move = (e: PointerEvent<SVGSVGElement>) => {
    const t = t0 + ((e.clientX - e.currentTarget.getBoundingClientRect().left - X0) / (X1 - X0)) * (t1 - t0)
    let best = 0
    pts.forEach((p, i) => { if (Math.abs(p[0] - t) < Math.abs(pts[best][0] - t)) best = i })
    setAt(best)
  }
  const key = (e: KeyboardEvent<SVGSVGElement>) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return
    e.preventDefault()
    setAt(Math.max(0, Math.min(pts.length - 1, (at ?? pts.length - 1) + (e.key === 'ArrowLeft' ? -1 : 1))))
  }

  return (
    <div className="chart" ref={box}>
      {head}
      <div>
        <div className="val">{s.fmt(cur[1])}</div>
        <div className="sub" aria-live="polite">
          {at != null ? when(cur[0]) : (
            <>
              {huge ? `up from ${s.fmt(first[1])} on ${day(t0 * 1000)}` : <><Change from={s.rate ? first[1] / 100 : first[1]} to={s.rate ? last[1] / 100 : last[1]} rate={s.rate} /> {range.word}</>}
              {avg && s.rate && <span className="pill">Avg {s.fmt(mean)}</span>}
            </>
          )}
        </div>
      </div>
      {width > 0 && (
        <svg width={W} height={H} role="img" tabIndex={0} onPointerMove={move} onPointerLeave={() => setAt(null)} onBlur={() => setAt(null)} onKeyDown={key}
          aria-label={`${s.label} from ${day(t0 * 1000)} to ${day(t1 * 1000)}: ${s.fmt(first[1])} to ${s.fmt(last[1])}. Arrow keys read each point.`}>
          <defs><linearGradient id={`g-${id}`} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="var(--accent-strong)" stopOpacity=".24" /><stop offset="1" stopColor="var(--accent-strong)" stopOpacity="0" /></linearGradient></defs>
          {tk.map((v) => <g key={v}><line className="gridl" x1={X0} x2={X1} y1={y(v)} y2={y(v)} /><text x={X1 + 8} y={y(v) + 4}>{tf(v)}</text></g>)}
          {marks(t0, t1, small ? 3 : 5).filter((m) => x(m.t) > X0 + 24 && x(m.t) < X1 - 24).map((m) => <text key={m.t} x={x(m.t)} y={H - 8} textAnchor="middle">{m.text}</text>)}
          <path d={`${d}L${X1} ${Y1}L${X0} ${Y1}Z`} fill={`url(#g-${id})`} />
          {avg && s.rate && <line className="avg" x1={X0} x2={X1} y1={y(mean)} y2={y(mean)} />}
          <path className="ln" d={d} />
          {at != null && <line className="cur" x1={x(cur[0])} x2={x(cur[0])} y1={Y0} y2={Y1} />}
          <circle r="9" fill="var(--accent-strong)" opacity=".22" cx={x(cur[0])} cy={y(cur[1])} />
          <circle r="4" fill="var(--accent-strong)" stroke="var(--card)" strokeWidth="2" cx={x(cur[0])} cy={y(cur[1])} />
        </svg>
      )}
      <div className="ctl">
        <span className="seg" role="group" aria-label="Range">{ranges.map((r, i) => <button key={r.label} type="button" aria-pressed={i === ri} onClick={() => { setRi(i); setAt(null) }}>{r.label}</button>)}</span>
        <span className="caption">{pts.length} readings, {hourly ? 'one an hour' : 'one a day'}</span>
      </div>
    </div>
  )
}

if (import.meta.env.DEV) {
  console.assert(ticks(1273, 1334, 4).join() === '1260,1280,1300,1320,1340', 'ticks')
  console.assert(marks(Date.UTC(2026, 10, 18, 2) / 1000, Date.UTC(2026, 10, 19, 2) / 1000, 5).map((m) => m.text).join() === '06:00,12:00,18:00,19 Nov', 'marks')
}
