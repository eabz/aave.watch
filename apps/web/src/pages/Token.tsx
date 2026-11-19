import { useMemo, useState } from 'react'
import { useApi } from '../api.ts'
import { Chart, type Point } from '../Chart.tsx'
import { ms, pct, price, short, usd } from '../format.ts'
import { chainName, hubKey, MOVES, num, sum } from '../lib.ts'
import { AssetTable, HolderTable, LimitRows } from '../tables.tsx'
import { useNow } from '../hooks.ts'
import { marketPath, poolPath } from '../lib.ts'
import { Card, Chain, ChainPill, Crumb, KindPill, Move, Region, Stamp, Stat, Token as TokenLogo } from '../ui.tsx'

const RANGES = [{ label: '1D', n: 25, word: 'in a day' }, { label: '1W', n: 169, word: 'this week' }, { label: '1M', n: 0, word: 'this month' }]
const rate = (v: number) => `${v.toFixed(2)}%`

export function Token({ symbol }: { symbol: string }) {
  const assets = useApi('/v1/hub-assets', {}, 30_000)
  const rows = useMemo(() => (assets.data?.data ?? []).filter((r) => String(r.symbol).toLowerCase() === symbol && (r.hub_name || num(r.added_usd) > 0)) /* an unnamed pool shows once it holds something */.sort((a, b) => num(b.added_usd) - num(a.added_usd)), [assets.data, symbol])
  const [picked, setPicked] = useState(0)
  const pool = rows[Math.min(picked, rows.length - 1)]
  const key = pool && hubKey(pool)
  const history = useApi(pool ? '/v1/history/hub-asset' : null, pool ? { chain: key.chain, hub: key.hub, asset_id: String(key.asset), hours: 720 } : {})
  const lines = useApi(pool ? '/v1/hub-spokes' : null, pool ? { chain: key.chain, hub: key.hub, asset_id: String(key.asset) } : {}, 60_000)
  const holders = useApi('/v1/holders', { symbol, limit: 10 }, 60_000)
  const events = useApi('/v1/events', { symbol, limit: 200 }, 30_000)
  const now = useNow()
  const points = useMemo(() => {
    const h = history.data?.data ?? []
    return { rate: h.map((r) => [ms(r.hour) / 1000, num(r.drawn_rate_apr) * 100] as Point), dep: h.map((r) => [ms(r.hour) / 1000, num(r.added_usd)] as Point) }
  }, [history.data])

  if (assets.data && rows.length === 0) return <><h1>Nothing on Aave v4 is called "{symbol}"</h1><p className="muted">Try the search, or see <a href="/rates">every asset</a>.</p></>
  if (!pool) return <h1 className="say">Reading {symbol.toUpperCase()}…</h1>

  const name = pool.symbol as string, dep = sum(rows, 'added_usd'), bor = sum(rows, 'owed_usd')
  const earning = rows.filter((r) => num(r.supply_apy_7d) > 0), lending = rows.filter((r) => num(r.drawn_rate_apr) > 0)
  const best = earning.reduce((a, r) => (num(r.supply_apy_7d) > num(a.supply_apy_7d) ? r : a), earning[0])
  const cheap = lending.reduce((a, r) => (num(r.drawn_rate_apr) < num(a.drawn_rate_apr) ? r : a), lending[0])
  const chains = [...new Set(rows.map((r) => hubKey(r).chain))]
  const borrowers = (lines.data?.data ?? []).filter((r) => num(r.owed_usd) > 0 || num(r.draw_cap) > 0).sort((a, b) => num(b.owed_usd) - num(a.owed_usd))

  return (
    <>
      <Crumb to="/rates" label="Rates" here={name} />
      <div className="phead">
        <TokenLogo chain={key.chain} token={pool.token} symbol={name} large bare /><h1>{name}</h1>
        <span className="pills"><KindPill symbol={name} />{chains.map((c) => <ChainPill key={c} id={c} />)}<span className="caption">Price {price(num(pool.price))}</span></span>
      </div>
      <div className="stats">
        <Stat label="Deposited" value={usd(dep)} at={pool}>in {rows.length} {rows.length === 1 ? 'pool' : 'pools'}</Stat>
        <Stat label="Borrowed" value={usd(bor)} at={pool}>{dep ? `${pct(bor / dep, 1)} lent out` : ''}</Stat>
        <Stat label="Best place to earn" value={best ? pct(best.supply_apy_7d) : 'Nowhere yet'} at={best}>{best && <Chain id={hubKey(best).chain} name={best.hub_name} href={poolPath(best)} />}</Stat>
        <Stat label="Cheapest to borrow" value={cheap ? pct(num(cheap.drawn_rate_apr)) : 'Not lent'} at={cheap}>{cheap && <Chain id={hubKey(cheap).chain} name={cheap.hub_name} href={poolPath(cheap)} />}</Stat>
      </div>
      <Card>
        <Chart id="token" key={`${key.chain}:${key.hub}`} title={`${name} in ${pool.hub_name || short(key.hub)} on ${chainName(key.chain)}`} avg hourly ranges={RANGES}
          tools={rows.length > 1 && (
            <select className="select" aria-label="Pool to chart" value={picked} onChange={(e) => setPicked(Number(e.target.value))}>
              {rows.map((r, i) => <option key={poolPath(r)} value={i}>{r.hub_name} on {chainName(hubKey(r).chain)}</option>)}
            </select>
          )}
          series={[{ label: 'Borrow rate', rate: true, fmt: rate, points: points.rate }, { label: 'Deposited', fmt: usd, points: points.dep }]} />
      </Card>
      <Card title={`Pools that hold ${name}`} note="Earn is measured over the last 7 days. From 90% lent out, taking money out can be slow.">
        <AssetTable rows={rows} first="pool" />
      </Card>
      <Card title="Markets that borrow it" tools={<Chain id={key.chain} name={pool.hub_name} href={poolPath(pool)} />} note="Each market can borrow up to its own limit from this pool.">
        <Region q={lines} what="the markets" empty="No market is connected to this pool for this token.">
          {() => borrowers.length === 0 ? <span className="muted">No market borrows {name} from this pool.</span>
            : <LimitRows rows={borrowers} label={(r) => <a className="name" href={marketPath(r)}>{r.spoke_name || short(r.spoke)}</a>} />}
        </Region>
      </Card>
      <div className="cols">
        <Card title={`Biggest holders of ${name}`} note="Every market and chain added up.">
          <Region q={holders} what="the holders" empty={`Nobody holds ${name} on Aave v4 yet.`}>{(hs) => <HolderTable rows={hs} />}</Region>
        </Card>
        <Card title={`Latest ${name} moves`}>
          <Region q={events} what="the moves" empty={`Nothing has moved in ${name} yet.`}>
            {(es) => { const moves = es.filter((e) => MOVES.includes(e.name)).slice(0, 8); return moves.length ? <div className="rows">{moves.map((e) => <Move key={`${e.chain_id}:${e.block_number}:${e.log_index}`} e={e} now={now} />)}</div> : <span className="muted">No deposits, loans or repayments lately.</span> }}
          </Region>
        </Card>
      </div>
      <Stamp rows={rows} />
    </>
  )
}
