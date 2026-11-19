import { useMemo } from 'react'
import { type Row, useApi } from '../api.ts'
import { limitUse } from '../caps.ts'
import { age, short, usd } from '../format.ts'
import { num, sum } from '../lib.ts'
import { LimitRows, LoanTable } from '../tables.tsx'
import { marketPath } from '../lib.ts'
import { Card, Chain, Region, Stamp, Stat, Token, Wallet } from '../ui.tsx'
import { useNow } from '../hooks.ts'
import { useThrottled, useTopic } from '../live.ts'

export function CloseCalls() {
  const now = useNow()
  const summary = useApi('/v1/summary', {}, 30_000)
  const close = useApi('/v1/positions', { max_hf: 1.1, limit: 1000 }, 15_000)
  const liqs = useApi('/v1/liquidations', { limit: 10 }, 60_000)
  const lines = useApi('/v1/hub-spokes', {}, 60_000)
  const spokes = useApi('/v1/spokes', { kind: 'spoke' })
  const assets = useApi('/v1/hub-assets')

  const s = summary.data?.data
  // a wallet changing safety band arrives on the stream: read the list again, at most every few seconds
  useTopic('risk', useThrottled(() => { close.refetch(); summary.refetch() }))
  const loans = useMemo(() => [...(close.data?.data ?? [])].sort((a, b) => num(b.debt_usd) - num(a.debt_usd)), [close.data])
  const full = useMemo(() => (lines.data?.data ?? []).filter((r) => (limitUse(r).draw ?? 0) >= 0.9 && num(r.owed_usd) > 0).sort((a, b) => num(b.owed_usd) - num(a.owed_usd)), [lines.data])
  const markets = useMemo(() => (spokes.data?.data ?? []).map((m): Row => {
    const mine = (lines.data?.data ?? []).filter((r) => r.spoke === m.address && Number(r.chain_id) === Number(m.chain_id))
    return { ...m, spoke: m.address, dep: sum(mine, 'added_usd'), bor: sum(mine, 'owed_usd') }
  }).filter((m) => m.dep + m.bor > 0).sort((a, b) => b.bor - a.bor), [spokes.data, lines.data])
  // deficit is ray-scaled; any digit that is not zero means bad debt
  const bad = (assets.data?.data ?? []).filter((r) => /[1-9]/.test(String(r.deficit_ray ?? '0')))

  return (
    <>
      <h1 className="visually-hidden">Close calls</h1>
      <div className="stats">
        <Stat label="Close to liquidation" value={s ? sum(s, 'positions_below_1_1').toLocaleString('en-US') : '–'} at={s?.[0]}>wallets under 9% away</Stat>
        <Stat label="Borrowed by them" value={s ? usd(sum(s, 'debt_below_1_1')) : '–'} at={s?.[0]}>across every market</Stat>
        <Stat label="Can be liquidated now" value={s ? sum(s, 'liquidatable') : '–'} at={s?.[0]}>wallets</Stat>
        <Stat label="Bad debt" value={assets.data ? (bad.length ? `${bad.length} assets` : 'None') : '–'}>{bad.length ? 'see the pools' : 'in any pool'}</Stat>
      </div>
      <Card title="Closest loans" note="Under 9% from liquidation, biggest loans first. A small loan can sit here for a long time: nobody rushes to liquidate a few cents.">
        <Region q={close} what="the loans" empty="All quiet. Nobody is close." hold={980}>
          {() => (
<LoanTable rows={loans} market />
          )}
        </Region>
      </Card>
      <div className="cols">
        <Card title="Latest liquidations">
          <Region q={liqs} what="the liquidations" empty="No liquidations yet.">
            {(rows) => <div className="rows">{rows.map((l) => (
              <div className="row row--2" key={l.tx_hash + l.log_index}>
                <span className="pills"><span className="pill c-red">Liquidated</span><b>{usd(num(l.debt_usd))}</b>{l.debt_symbol && <Token chain={l.chain_id} token={l.debt_token} symbol={l.debt_symbol} />}<Wallet address={l.user} /></span>
                <span className="muted">{age(l.block_time, now)}</span>
              </div>
            ))}</div>}
          </Region>
        </Card>
        <Card title="Markets" note="Where people deposit and borrow. Biggest borrowers first.">
          <Region q={spokes} what="the markets" empty="No markets yet.">
            {() => <div className="rows">{markets.slice(0, 12).map((m) => (
              <div className="row" key={m.chain_id + m.address}><Chain id={m.chain_id} name={m.name || short(m.address)} href={marketPath(m)} /><span>{usd(m.bor)} <span className="muted">borrowed</span></span></div>
            ))}</div>}
          </Region>
        </Card>
      </div>
      <Card title="Limits nearly full" note="A market can only borrow so much of each asset from a pool. From 90% it is nearly full; at 100% nobody can borrow more there.">
        <Region q={lines} what="the limits" empty="No limits yet.">
          {() => full.length === 0 ? <span className="muted">No market is near a limit.</span>
            : <LimitRows rows={full.slice(0, 15)} label={(r) => <span className="pills"><Chain id={r.chain_id} name={r.spoke_name || short(r.spoke)} href={marketPath(r)} /><Token chain={r.chain_id} symbol={r.symbol} /></span>} />}
        </Region>
      </Card>
      <Stamp rows={[...(s ?? []), ...loans.slice(0, 3)]} />
    </>
  )
}
