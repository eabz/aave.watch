import { useMemo } from 'react'
import { type Row, useApi } from '../api.ts'
import { Chart, type Point } from '../Chart.tsx'
import { band, distance, ms, pct, short, usd, usdExact } from '../format.ts'
import { away, MOVES, num, sum } from '../lib.ts'
import { BandPill, Card, ChainPill, CopyButton, Explorer, Move, Region, Stamp, Stat, Token } from '../ui.tsx'
import { useNow } from '../hooks.ts'
import { useTopic } from '../live.ts'

type Account = Row & { reserves: Row[] }
const RANGES = [{ label: '1W', n: 8, word: 'this week' }, { label: '1M', n: 31, word: 'in a month' }, { label: 'All', n: 0, word: 'since its first deposit' }]

export function Wallet({ address }: { address: string }) {
  const now = useNow()
  // the address may be on any of the four chains
  const a1 = useApi<{ data: Account[] }>('/v1/account', { chain: 1, user: address }, 30_000)
  const a2 = useApi<{ data: Account[] }>('/v1/account', { chain: 5042, user: address }, 30_000)
  const a3 = useApi<{ data: Account[] }>('/v1/account', { chain: 43114, user: address }, 30_000)
  const a4 = useApi<{ data: Account[] }>('/v1/account', { chain: 8453, user: address }, 30_000)
  const events = useApi('/v1/events', { user: address, limit: 100 })
  // when this wallet moves, read it again
  useTopic('feed', (e) => { if (e.data?.user === address) { for (const q of [a1, a2, a3, a4, events]) q.refetch() } })
  const queries = [a1, a2, a3, a4]
  const ready = queries.every((q) => q.data || q.error)
  const accounts = useMemo(() => !ready ? [] : queries.flatMap((q) => q.data?.data ?? []).filter((a) => num(a.collateral_usd) + num(a.debt_usd) > 0).sort((a, b) => num(b.debt_usd) - num(a.debt_usd) || num(b.collateral_usd) - num(a.collateral_usd)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ready, a1.data, a2.data, a3.data, a4.data])
  const loans = accounts.filter((a) => a.health_factor != null && num(a.debt_usd) > 0)
  const worst = loans.length ? loans.reduce((a, b) => (b.health_factor < a.health_factor ? b : a)) : null
  const history = useApi(worst ? '/v1/history/account' : null, worst ? { chain: worst.chain_id, user: address, spoke: worst.spoke, step: 'day', hours: 24 * 366 * 5 } : {})
  const points = useMemo(() => {
    const h = history.data?.data ?? []
    return { away: h.filter((r) => r.health_factor != null).map((r) => [ms(r.hour) / 1000, (distance(r.health_factor) ?? 0) * 100] as Point), debt: h.map((r) => [ms(r.hour) / 1000, num(r.debt_usd)] as Point), dep: h.map((r) => [ms(r.hour) / 1000, num(r.collateral_usd)] as Point) }
  }, [history.data])
  const moves = (events.data?.data ?? []).filter((e) => MOVES.includes(e.name)).slice(0, 8)
  const chains = [...new Set(accounts.map((a) => Number(a.chain_id)))]
  const dep = sum(accounts, 'collateral_usd'), bor = sum(accounts, 'debt_usd')
  const d = worst && distance(worst.health_factor)

  return (
    <>
      <div className="pills">
        <span className="mono" title={address}>{short(address)}</span><CopyButton value={address} />
        {chains.map((c) => <span className="pills" key={c}><ChainPill id={c} /><Explorer chain={c} address={address} /></span>)}
        {worst && <BandPill hf={worst.health_factor} />}
      </div>
      <h1 className="visually-hidden">Wallet {short(address)}</h1>
      {!ready ? <div className="state muted" aria-busy="true" style={{ minHeight: '80vh' }}>Reading this wallet…</div> : accounts.length === 0 && <p>This wallet has nothing in Aave v4.</p>}
      {worst && d != null && (
        <Card note={`This is the loan in ${worst.spoke_name || short(worst.spoke)}${loans.length > 1 ? `, the closest of ${loans.length}` : ''}.`}>
          <div className="gauge" role="img" aria-label={`${pct(d, 1)} from liquidation, in the ${band(worst.health_factor)} band`}>
            <i style={{ left: `${Math.min(100, d * 200)}%` }} /><b style={{ left: `${Math.max(8, Math.min(92, d * 200))}%` }}>{d === 0 ? 'Can be liquidated now' : `${pct(d, 1)} away`}</b>
          </div>
          <div className="gscale"><span style={{ left: 0 }}>0%</span><span style={{ left: '18%' }}>9%</span><span style={{ left: '66%' }}>33%</span><span className="end" style={{ left: '100%' }}>50% or more</span>
            <em style={{ left: '9%' }}>Close</em><em style={{ left: '42%' }}>Watch</em><em style={{ left: '83%' }}>Safe</em></div>
        </Card>
      )}
      {accounts.length > 0 && (
        <div className="stats">
          <Stat label="Deposited" value={usd(dep)} at={accounts[0]}>in {accounts.length} {accounts.length === 1 ? 'market' : 'markets'}</Stat>
          <Stat label="Borrowed" value={usd(bor)} at={accounts[0]}>{bor ? `${pct(bor / dep, 1)} of deposits` : 'nothing'}</Stat>
          <Stat label="Deposits minus loans" value={usd(dep - bor)} at={accounts[0]} />
        </div>
      )}
      {worst && !history.data && !history.error && <Card><div className="state chart" aria-busy="true"><span className="skel" style={{ width: '40%' }} /></div></Card>}
      {worst && points.debt.length > 1 && (
        <Card>
          <Chart id="wallet" title={`This wallet in ${worst.spoke_name || short(worst.spoke)}`} ranges={RANGES}
            series={[{ label: 'From liquidation', rate: true, fmt: (v) => `${v.toFixed(1)}%`, points: points.away }, { label: 'Borrowed', fmt: usd, points: points.debt }, { label: 'Deposited', fmt: usd, points: points.dep }]} />
        </Card>
      )}
      <div className="cols">
        {accounts.map((a) => (
          <Card key={`${a.chain_id}:${a.spoke}`} title={<a className="name" href={`/market/${a.chain_id}/${a.spoke}`}>{a.spoke_name || short(a.spoke)}</a>}
            tools={<span className="pills"><span className="caption">{a.health_factor == null ? '' : `${away(a.health_factor)} away`}</span><BandPill hf={num(a.debt_usd) > 0 ? a.health_factor : null} /></span>}>
            <div className="rows">
              {a.reserves.filter((r) => num(r.supplied_usd) > 0).map((r) => <div className="row" key={`s${r.reserve_id}`}><span className="pills"><span className="pill">Deposited</span><Token chain={a.chain_id} symbol={r.symbol} /></span><b>{usdExact(num(r.supplied_usd))}</b></div>)}
              {a.reserves.filter((r) => num(r.debt_usd) > 0).map((r) => <div className="row" key={`d${r.reserve_id}`}><span className="pills"><span className="pill">Borrowed</span><Token chain={a.chain_id} symbol={r.symbol} /></span><b>{usdExact(num(r.debt_usd))}</b></div>)}
            </div>
          </Card>
        ))}
      </div>
      <Card title="Recent moves">
        <Region q={events} what="this wallet's moves" empty="This wallet has not moved anything on Aave v4.">
          {() => moves.length === 0 ? <span className="muted">No deposits, loans or repayments yet.</span> : <div className="rows">{moves.map((e) => <Move key={`${e.chain_id}:${e.block_number}:${e.log_index}`} e={e} now={now} />)}</div>}
        </Region>
      </Card>
      <Stamp rows={accounts} />
    </>
  )
}
