import { useMemo, useState } from 'react'
import { useApi } from '../api.ts'
import { age, band, pct, short, usd } from '../format.ts'
import { chainName, num, sum } from '../lib.ts'
import { LoanTable } from '../tables.tsx'
import { Card, ChainPill, Crumb, Region, SortTh, Stamp, Stat, Token, Wallet } from '../ui.tsx'
import { useNow, useSort } from '../hooks.ts'
import { useThrottled, useTopic } from '../live.ts'

export function Market({ chain, spoke }: { chain: number; spoke: string }) {
  const now = useNow()
  const spokes = useApi('/v1/spokes', { chain })
  const reserves = useApi('/v1/reserves', { chain, spoke }, 30_000)
  const loans = useApi('/v1/positions', { chain, spoke, max_hf: 1.5, limit: 1000 }, 15_000)
  const lines = useApi('/v1/hub-spokes', { chain, spoke })
  const liqs = useApi('/v1/liquidations', { chain, spoke, limit: 5 }, 60_000)
  const [all, setAll] = useState(false)
  useTopic('risk', useThrottled(() => loans.refetch()))

  const me = spokes.data?.data.find((s) => s.address === spoke)
  const rows = useMemo(() => [...(reserves.data?.data ?? [])].filter((r) => num(r.supplied_usd) + num(r.debt_usd) > 0).sort((a, b) => num(b.supplied_usd) - num(a.supplied_usd)), [reserves.data])
  const watched = useMemo(() => loans.data?.data ?? [], [loans.data])
  const bands = useMemo(() => {
    const of = (b: string) => watched.filter((r) => band(r.health_factor) === b)
    return { now: of('now'), close: of('close'), watch: of('watch') }
  }, [watched])
  const biggest = useMemo(() => [...watched].sort((a, b) => num(b.debt_usd) - num(a.debt_usd)).slice(0, 10), [watched])
  const table = useSort(rows, { name: (r) => String(r.symbol).toLowerCase(), dep: (r) => num(r.supplied_usd), bor: (r) => num(r.debt_usd), cf: (r) => num(r.collateral_factor), bonus: (r) => num(r.max_liquidation_bonus) }, { key: 'dep', dir: 'descending' })
  const th = { sort: table.sort, toggle: table.toggle }
  const pools = [...new Map((lines.data?.data ?? []).map((r) => [r.hub, r])).values()]

  if (spokes.data && !me) return <><h1>There is no market here</h1><p className="muted">No market of Aave v4 has this address on {chainName(chain)}. See <a href="/close-calls">the markets</a>.</p></>
  if (!me || !reserves.data) return <h1 className="say">Reading the market…</h1>

  const name = me.name || short(spoke), dep = sum(rows, 'supplied_usd'), bor = sum(rows, 'debt_usd')
  const closeUsd = sum(bands.close, 'debt_usd') + sum(bands.now, 'debt_usd'), watchUsd = sum(bands.watch, 'debt_usd'), safeUsd = Math.max(0, bor - closeUsd - watchUsd)
  const part = (v: number) => `${bor ? (v / bor) * 100 : 0}%`

  return (
    <>
      <Crumb to="/close-calls" label="Close calls" here={`${name} on ${chainName(chain)}`} />
      <div className="phead">
        <h1>{name}</h1>
        <span className="pills"><ChainPill id={chain} /><span className="pill">Market (Spoke)</span>
          {pools.length > 0 && <span className="caption">Borrows from {pools.map((p, i) => <span key={p.hub}>{i > 0 && ', '}<a className="name" href={`/pool/${chain}/${p.hub}`}>{p.hub_name ?? short(p.hub)}</a></span>)}</span>}
        </span>
      </div>
      <div className="stats">
        <Stat label="Deposited" value={usd(dep)} at={rows[0]}>{rows.length} assets</Stat>
        <Stat label="Borrowed" value={usd(bor)} at={rows[0]}>{pct(dep ? bor / dep : 0, 1)} of deposits</Stat>
        <Stat label="Close to liquidation" value={bands.close.length} at={bands.close[0]}>wallets, {usd(sum(bands.close, 'debt_usd'))} borrowed</Stat>
        <Stat label="Can be liquidated now" value={bands.now.length} at={bands.now[0]}>{bands.now.length ? `wallets, ${usd(sum(bands.now, 'debt_usd'))} borrowed` : 'nobody'}</Stat>
      </div>
      {bor > 0 && (
        <Card title="How safe are the loans" note="By how far each loan is from liquidation. The safe part is everything borrowed that is not in the other two.">
          <div className="stack" role="img" aria-label={`Loans by distance from liquidation: close ${part(closeUsd)}, watch ${part(watchUsd)}, safe ${part(safeUsd)}`}>
            <s className="fill c-red" style={{ width: part(closeUsd) }} /><s className="fill c-amber" style={{ width: part(watchUsd) }} /><s className="fill c-green" style={{ width: part(safeUsd) }} />
          </div>
          <div className="legend">
            <span><i className="swt fill c-red" />Close, under 9% away · {usd(closeUsd)} · {bands.close.length + bands.now.length} wallets</span>
            <span><i className="swt fill c-amber" />Watch, 9% to 33% away · {usd(watchUsd)} · {bands.watch.length} wallets</span>
            <span><i className="swt fill c-green" />Safe, over 33% away · {usd(safeUsd)}</span>
          </div>
        </Card>
      )}
      <Card title="What can be used here" note={'"Counts for 84%" means $100 of it backs loans as $84. The bonus is what a liquidator earns for closing a loan that got too close.'}>
        <div className="scroll"><table>
          <thead><tr><SortTh k="name" {...th}>Asset</SortTh><SortTh k="dep" r {...th}>Deposited</SortTh><SortTh k="bor" r {...th}>Borrowed</SortTh><th scope="col">Role</th><SortTh k="cf" r {...th}><span className="tip" title="The protocol calls this the collateral factor.">Counts for</span></SortTh><SortTh k="bonus" r {...th}>Liquidator's bonus, up to</SortTh></tr></thead>
          <tbody>{(all ? table.sorted : table.sorted.slice(0, 8)).map((r) => {
            const cf = num(r.collateral_factor) / 10_000, bonus = num(r.max_liquidation_bonus) / 10_000 - 1
            return (
              <tr key={r.reserve_id}>
                <td><Token chain={chain} token={r.token} symbol={r.symbol} /></td><td className="r">{usd(num(r.supplied_usd))}</td>
                <td className={`r${r.borrowable ? '' : ' muted'}`}>{r.borrowable ? usd(num(r.debt_usd)) : 'not lent'}</td>
                <td><span className="pills">{cf > 0 && <span className="pill">Collateral</span>}{!!r.borrowable && <span className="pill">Borrow</span>}{!!r.paused && <span className="pill c-amber">Paused</span>}{!!r.frozen && <span className="pill c-amber">Frozen</span>}</span></td>
                <td className={`r${cf > 0 ? '' : ' muted'}`}>{cf > 0 ? pct(cf, 0) : 'not collateral'}</td>
                <td className={`r${bonus > 0 ? '' : ' muted'}`}>{bonus > 0 ? pct(bonus) : 'none'}</td>
              </tr>
            )
          })}</tbody>
        </table></div>
        {rows.length > 8 && !all && <div><button className="btn" type="button" onClick={() => setAll(true)}>Show all {rows.length} assets</button></div>}
      </Card>
      <div className="cols">
        <Card title="Biggest loans to watch" note="Loans under 33% from liquidation, biggest first.">
          <Region q={loans} what="the loans" empty="All quiet here. Every loan is safe.">{() => <LoanTable rows={biggest} />}</Region>
        </Card>
        <Card title="Latest liquidations">
          <Region q={liqs} what="the liquidations" empty="Nobody has been liquidated in this market.">
            {(ls) => <div className="rows">{ls.map((l) => (
              <div className="row" key={l.tx_hash + l.log_index}>
                <span className="pills"><span className="pill c-red">Liquidated</span><b>{usd(num(l.debt_usd))}</b>{l.debt_symbol && <Token chain={chain} token={l.debt_token} symbol={l.debt_symbol} />}<Wallet address={l.user} /></span>
                <span className="muted">{age(l.block_time, now)}</span>
              </div>
            ))}</div>}
          </Region>
        </Card>
      </div>
      <Stamp rows={[...rows.slice(0, 3), ...watched.slice(0, 3)]} />
    </>
  )
}
