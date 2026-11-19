import { useMemo, useState } from 'react'
import { type Row, useApi } from '../api.ts'
import { Chart, type Point } from '../Chart.tsx'
import { ms, pct, usd } from '../format.ts'
import { CHAINS, MOVES, num, sum } from '../lib.ts'
import { AssetTable } from '../tables.tsx'
import { Card, Chain, Change, Move, Region, Stamp, Stat } from '../ui.tsx'
import { useNow } from '../hooks.ts'
import { useThrottled, useTopic } from '../live.ts'

const RANGES = [{ label: '1W', n: 8, word: 'this week' }, { label: '1M', n: 31, word: 'in a month' }, { label: 'All', n: 0, word: 'since launch' }]
const usdTick = (max: number) => (v: number) => (!v ? '$0' : max >= 1e9 ? `$${(v / 1e9).toFixed(2)}B` : usd(v))
const STRENGTH = [1, 0.7, 0.45, 0.25]

export function Now() {
  const now = useNow()
  const summary = useApi('/v1/summary', {}, 30_000)
  const totals = useApi('/v1/history/totals', { step: 'day', hours: 24 * 366 * 5 })
  const v3 = useApi('/v1/history/v3-totals', { step: 'day', hours: 48 })
  const assets = useApi('/v1/hub-assets', {}, 30_000)
  const events = useApi('/v1/events', { limit: 500 }, 30_000)

  const s = summary.data?.data
  // moves arrive on the stream as they happen; the totals are read again at most every few seconds
  const [fresh, setFresh] = useState<Row[]>([])
  const refresh = useThrottled(() => { summary.refetch(); assets.refetch() })
  useTopic('feed', (e) => { if (e.data && MOVES.includes(e.name ?? '')) { setFresh((f) => [{ ...e.data, name: e.name, chain_id: e.chain_id, block_number: e.block, log_index: e.log_index }, ...f].slice(0, 200)); refresh() } })
  const dep = s && sum(s, 'supplied_usd'), bor = s && sum(s, 'borrowed_usd')
  // one point a day, all chains added up; the strip and the chart's 1W both read the last eight
  const series = useMemo(() => {
    const byHour = new Map<string, [number, number]>()
    for (const r of totals.data?.data ?? []) { const p = byHour.get(r.hour) ?? [0, 0]; p[0] += num(r.supplied_usd); p[1] += num(r.borrowed_usd); byHour.set(r.hour, p) }
    const rows = [...byHour].filter(([, p]) => p[0] > 0).map(([h, p]) => [ms(h) / 1000, p[0], p[1]] as const)
    return { dep: rows.map((r) => [r[0], r[1]] as Point), bor: rows.map((r) => [r[0], r[2]] as Point) }
  }, [totals.data])
  const week = (pts: Point[]) => (pts.length > 8 ? <Change from={pts[pts.length - 8][1]} to={pts[pts.length - 1][1]} word="this week" /> : null)
  const chains = useMemo(() => CHAINS.map((id) => {
    const rows = (totals.data?.data ?? []).filter((r) => Number(r.chain_id) === id)
    return { id, now: num(rows[rows.length - 1]?.supplied_usd), before: num(rows[rows.length - 8]?.supplied_usd) }
  }).sort((a, b) => b.now - a.now), [totals.data])
  const total = chains.reduce((a, c) => a + c.now, 0)
  const v3rows = v3.data?.data ?? [], v3hour = v3rows[v3rows.length - 1]?.hour
  const v3dep = sum(v3rows.filter((r) => r.hour === v3hour), 'supplied_usd')
  const moves = useMemo(() => [...new Map([...fresh, ...(events.data?.data ?? [])].map((e) => [`${e.chain_id}:${e.block_number}:${e.log_index}`, e])).values()].filter((e) => MOVES.includes(e.name) && num(e.amount_usd) > 0)
    .sort((a, b) => num(b.amount_usd) - num(a.amount_usd)).slice(0, 5).sort((a, b) => ms(b.block_time) - ms(a.block_time)), [events.data, fresh])

  return (
    <>
      <h1 className="visually-hidden">Aave v4 now</h1>
      <div className="stats">
        <Stat label="Deposited" value={usd(dep)} at={s?.[0]}>{week(series.dep)}</Stat>
        <Stat label="Borrowed" value={usd(bor)} at={s?.[0]}>{week(series.bor)}</Stat>
        <Stat label="Lent out" value={dep ? pct(bor! / dep, 1) : '–'} at={s?.[0]}>of what is deposited</Stat>
        <Stat label="v4 share of Aave" value={dep && v3dep ? pct(dep / (dep + v3dep), 1) : '–'} at={v3rows[v3rows.length - 1]}>{v3dep ? `v3 still holds ${usd(v3dep)}` : ''}</Stat>
        <Stat label="Close to liquidation" value={s ? sum(s, 'positions_below_1_1').toLocaleString('en-US') : '–'} at={s?.[0]}>
          <a href="/close-calls">wallets, {s ? usd(sum(s, 'debt_below_1_1')) : ''} borrowed</a>
        </Stat>
      </div>
      <Card>
        <Chart id="now" title="Money in Aave v4" ranges={RANGES}
          series={[{ label: 'Deposited', fmt: usd, tick: usdTick, points: series.dep }, { label: 'Borrowed', fmt: usd, tick: usdTick, points: series.bor }]} />
      </Card>
      <div className="cols">
        <Card title="Where the money is" note="Deposits, with the change over 7 days.">
          <div className="stack" role="img" aria-label={`Deposits by chain: ${chains.map((c) => `${c.id} ${pct(total ? c.now / total : 0, 1)}`).join(', ')}`}>
            {chains.map((c, i) => <s key={c.id} className="fill" style={{ width: `${total ? (c.now / total) * 100 : 0}%`, opacity: STRENGTH[i] }} />)}
          </div>
          <div className="rows">
            {chains.map((c, i) => (
              <div className="row" key={c.id}>
                <span className="pills"><i className="swt fill" style={{ opacity: STRENGTH[i] }} /><Chain id={c.id} /></span>
                <span className="pills"><b>{usd(c.now)}</b><Change from={c.before} to={c.now} /></span>
              </div>
            ))}
          </div>
        </Card>
        <Card title="Big moves" note="The five biggest of the latest moves, updated as they happen.">
          <Region q={events} what="the latest moves" empty="Nothing has moved yet.">{() => <div className="rows">{moves.map((e: Row) => <Move key={`${e.chain_id}:${e.block_number}:${e.log_index}`} e={e} now={now} />)}</div>}</Region>
        </Card>
      </div>
      <Card title="Biggest assets" tools={<a className="small" href="/rates">All rates</a>}>
        <Region q={assets} what="the assets" empty="No assets yet.">{(rows) => <AssetTable rows={[...rows].sort((a, b) => num(b.added_usd) - num(a.added_usd)).slice(0, 6)} step={6} />}</Region>
      </Card>
      <Stamp rows={[...(s ?? []), ...(assets.data?.data ?? []).slice(0, 5)]} />
    </>
  )
}
