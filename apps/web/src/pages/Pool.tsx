import { useMemo } from 'react'
import { type Row, useApi } from '../api.ts'
import { Chart, type Point } from '../Chart.tsx'
import { ms, pct, short, usd } from '../format.ts'
import { chainName, type Kind, kind, KIND_CLASS, KINDS, num, sum } from '../lib.ts'
import { AssetTable } from '../tables.tsx'
import { marketPath } from '../lib.ts'
import { Card, ChainPill, Crumb, Region, Stamp, Stat, Used } from '../ui.tsx'

const RANGES = [{ label: '1W', n: 8, word: 'this week' }, { label: '1M', n: 31, word: 'in a month' }, { label: 'All', n: 0, word: 'since it opened' }]
const KIND_NAME: Record<Kind, string> = { eth: 'ETH and staked ETH', dollar: 'Dollars', btc: 'BTC', money: 'Euro, gold', other: 'Others' }

export function Pool({ chain, hub }: { chain: number; hub: string }) {
  const assets = useApi('/v1/hub-assets', { chain, hub }, 30_000)
  const lines = useApi('/v1/hub-spokes', { chain, hub }, 60_000)
  const history = useApi('/v1/history/totals', { chain, hub, step: 'day', hours: 24 * 366 * 5 })
  const points = useMemo(() => { const h = (history.data?.data ?? []).filter((r) => num(r.supplied_usd) > 0); return { dep: h.map((r) => [ms(r.hour) / 1000, num(r.supplied_usd)] as Point), bor: h.map((r) => [ms(r.hour) / 1000, num(r.borrowed_usd)] as Point) } }, [history.data])
  const rows = useMemo(() => [...(assets.data?.data ?? [])].filter((r) => num(r.added_usd) > 0).sort((a, b) => num(b.added_usd) - num(a.added_usd)), [assets.data])
  const markets = useMemo(() => {
    const by = new Map<string, Row[]>()
    for (const r of lines.data?.data ?? []) by.set(r.spoke, [...(by.get(r.spoke) ?? []), r])
    return [...by.values()].map((rs) => ({ first: rs[0], dep: sum(rs, 'added_usd'), bor: sum(rs, 'owed_usd') })).filter((m) => m.dep + m.bor >= 1).sort((a, b) => b.dep + b.bor - (a.dep + a.bor))
  }, [lines.data])

  if (assets.data && rows.length === 0) return <><h1>There is no pool here</h1><p className="muted">Nothing is deposited at this address on {chainName(chain)}. See <a href="/rates">every pool</a>.</p></>
  if (!rows.length) return <h1 className="say">Reading the pool…</h1>

  const name = (rows[0].hub_name || short(hub)) as string, dep = sum(rows, 'added_usd'), bor = sum(rows, 'owed_usd')
  const kinds = KINDS.map((k) => ({ k, usd: sum(rows.filter((r) => kind(r.symbol) === k), 'added_usd') })).filter((x) => x.usd > 0).sort((a, b) => b.usd - a.usd)
  const bad = rows.filter((r) => /[1-9]/.test(String(r.deficit_ray ?? '0')))
  const top = markets[0]

  return (
    <>
      <Crumb to="/rates" label="Rates" here={`${name} on ${chainName(chain)}`} />
      <div className="phead">
        <h1>{name}</h1>
        <span className="pills"><ChainPill id={chain} /><span className="pill">Pool (Hub)</span>{bad.length ? <span className="pill c-red">Bad debt in {bad.map((r) => r.symbol).join(', ')}</span> : <span className="pill c-green">No bad debt</span>}</span>
      </div>
      <div className="stats">
        <Stat label="Deposited" value={usd(dep)} at={rows[0]}>{rows.length} assets</Stat>
        <Stat label="Borrowed" value={usd(bor)} at={rows[0]}>{pct(dep ? bor / dep : 0, 1)} lent out</Stat>
        <Stat label="Biggest asset" value={rows[0].symbol} at={rows[0]}>{pct(num(rows[0].added_usd) / dep, 1)} of the pool</Stat>
        <Stat label="Biggest market" value={top ? top.first.spoke_name || short(top.first.spoke) : '–'} at={top?.first}>{top ? `${usd(top.dep)} deposited through it` : ''}</Stat>
      </div>
      <Card>
        <Chart id="pool" title={`Money in ${name}`} ranges={RANGES} series={[{ label: 'Deposited', fmt: usd, points: points.dep }, { label: 'Borrowed', fmt: usd, points: points.bor }]} />
      </Card>
      <Card title="What is inside">
        <div className="stack" role="img" aria-label={`Pool by kind of asset: ${kinds.map((x) => `${KIND_NAME[x.k]} ${pct(x.usd / dep, 1)}`).join(', ')}`}>
          {kinds.map((x) => <s key={x.k} className={`fill ${KIND_CLASS[x.k]}`} style={{ width: `${(x.usd / dep) * 100}%`, ...(x.k === 'other' ? { background: 'var(--ink-2)' } : {}) }} />)}
        </div>
        <div className="legend">{kinds.map((x) => <span key={x.k}><i className={`swt fill ${KIND_CLASS[x.k]}`} style={x.k === 'other' ? { background: 'var(--ink-2)' } : undefined} />{KIND_NAME[x.k]} · {usd(x.usd)} · {pct(x.usd / dep, 1)}</span>)}</div>
      </Card>
      <Card title="Assets" note="Earn is measured over the last 7 days. Borrow is the rate right now.">
        <AssetTable rows={rows} pool={false} step={8} />
      </Card>
      <Card title="Markets plugged into this pool" note="Deposits come in through a market, and each market lends some of the pool out.">
        <Region q={lines} what="the markets" empty="No market is connected to this pool.">
          {() => (
            <div className="scroll"><table>
              <thead><tr><th scope="col">Market</th><th scope="col" className="r">Deposited through it</th><th scope="col" className="r">Borrowed</th><th scope="col" className="r">Share of the pool's loans</th></tr></thead>
              <tbody>{markets.map((m) => (
                <tr key={m.first.spoke}>
                  <td>{m.first.spoke_kind === 'spoke' ? <a className="name" href={marketPath(m.first)}>{m.first.spoke_name || short(m.first.spoke)}</a> : <span className="strong">{m.first.spoke_name || short(m.first.spoke)}</span>}</td>
                  <td className="r">{usd(m.dep)}</td><td className="r">{usd(m.bor)}</td><td><Used value={bor ? m.bor / bor : 0} plain /></td>
                </tr>
              ))}</tbody>
            </table></div>
          )}
        </Region>
      </Card>
      <Stamp rows={rows.slice(0, 5)} />
    </>
  )
}
