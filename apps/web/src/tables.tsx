import { type ReactNode, useState } from 'react'
import type { Row } from './api.ts'
import { limitUse } from './caps.ts'
import { distance, pct, usd } from './format.ts'
import { useSort } from './hooks.ts'
import { away, hubKey, marketPath, num, poolPath } from './lib.ts'
import { BandPill, Chain, SortTh, Token, Used, Wallet } from './ui.tsx'

/** "Show more" under a long table: `step` rows at a time. */
function More({ total, shown, step, set }: { total: number; shown: number; step: number; set: (n: number) => void }) {
  return total > shown ? <div><button className="btn" type="button" onClick={() => set(shown + step)}>Show more ({total - shown} left)</button></div> : null
}

/** Rows of pool assets (`/v1/hub-assets`). `first` picks what names the row: the asset, or the pool that holds it. */
export function AssetTable({ rows, first = 'asset', pool = true, step = 25 }: { rows: Row[]; first?: 'asset' | 'pool'; pool?: boolean; step?: number }) {
  const [shown, setShown] = useState(step)
  const { sorted, sort, toggle } = useSort(rows, {
    name: (r) => String(first === 'asset' ? r.symbol : r.hub_name).toLowerCase(), dep: (r) => num(r.added_usd), earn: (r) => num(r.supply_apy_7d),
    rate: (r) => num(r.drawn_rate_apr), bor: (r) => num(r.owed_usd), pool: (r) => String(r.hub_name).toLowerCase(), used: (r) => num(r.usage),
  }, { key: 'dep', dir: 'descending' })
  const name = (r: Row) => <Chain id={hubKey(r).chain} name={r.hub_name} href={poolPath(r)} />
  const th = { sort, toggle }
  return (
    <>
      <div className="scroll">
        <table>
          <thead><tr>
            <SortTh k="name" {...th}>{first === 'asset' ? 'Asset' : 'Pool'}</SortTh><SortTh k="dep" r {...th}>Deposited</SortTh><SortTh k="earn" r {...th}>Earn, 7 days</SortTh><SortTh k="rate" r {...th}>Borrow now</SortTh>
            {first === 'asset' && pool ? <SortTh k="pool" {...th}>Pool</SortTh> : <SortTh k="bor" r {...th}>Borrowed</SortTh>}<SortTh k="used" r {...th}>Lent out</SortTh>
          </tr></thead>
          <tbody>
            {sorted.slice(0, shown).map((r) => {
              const k = hubKey(r), idle = num(r.owed_usd) < 1 && !num(r.supply_apy_7d)
              return (
                <tr key={`${k.chain}:${k.hub}:${k.asset}`}>
                  <td>{first === 'asset' ? <Token chain={k.chain} token={r.token} symbol={r.symbol} /> : name(r)}</td>
                  <td className="r">{usd(num(r.added_usd))}</td>
                  {idle ? <td className="muted" colSpan={2}>Not lent out, so it earns nothing</td> : <><td className="r">{pct(r.supply_apy_7d)}</td><td className="r">{pct(num(r.drawn_rate_apr))}</td></>}
                  {first === 'asset' && pool ? <td>{name(r)}</td> : <td className="r">{idle ? '' : usd(num(r.owed_usd))}</td>}
                  <td>{idle ? null : <Used value={num(r.usage)} />}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
      <More total={sorted.length} shown={shown} step={step} set={setShown} />
    </>
  )
}

/** What each market has borrowed of one pool asset, against its limit (`/v1/hub-spokes` rows). */
export function LimitRows({ rows, label }: { rows: Row[]; label: (r: Row) => ReactNode }) {
  const { sorted, sort, toggle } = useSort(rows, { bor: (r) => num(r.owed_usd), cap: (r) => (limitUse(r).drawCap ?? Infinity) * num(r.price), used: (r) => limitUse(r).draw ?? -1 }, { key: 'bor', dir: 'descending' })
  const th = { sort, toggle }
  return (
    <div className="scroll">
      <table>
        <thead><tr><th scope="col">Market</th><SortTh k="bor" r {...th}>Borrowed</SortTh><SortTh k="cap" r {...th}>Its limit</SortTh><SortTh k="used" r {...th}>Limit used</SortTh></tr></thead>
        <tbody>
          {sorted.map((r) => {
            const u = limitUse(r)
            return (
              <tr key={`${r.chain_id}:${r.hub}:${r.asset_id}:${r.spoke}`}>
                <td>{label(r)}</td><td className="r">{usd(num(r.owed_usd))}</td>
                <td className="r">{u.drawCap == null ? <span className="muted">none</span> : usd(u.drawCap * num(r.price))}</td>
                <td><Used value={u.draw} /></td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

/** Wallets with a loan (`/v1/positions` rows): who, how much, and how far from liquidation. */
export function LoanTable({ rows, market, step = 20 }: { rows: Row[]; market?: boolean; step?: number }) {
  const [shown, setShown] = useState(step)
  const { sorted, sort, toggle } = useSort(rows, { bor: (r) => num(r.debt_usd), away: (r) => distance(r.health_factor) ?? Infinity, market: (r) => String(r.spoke_name || '~').toLowerCase() }, { key: 'bor', dir: 'descending' })
  const th = { sort, toggle }
  return (
    <>
      <div className="scroll">
        <table>
          <thead><tr><th scope="col">Wallet</th><SortTh k="bor" r {...th}>Borrowed</SortTh><SortTh k="away" r {...th}>From liquidation</SortTh><th scope="col"><span className="visually-hidden">Safety</span></th>{market && <SortTh k="market" {...th}>Market</SortTh>}</tr></thead>
          <tbody>
            {sorted.slice(0, shown).map((r) => (
              <tr key={`${r.chain_id}:${r.spoke}:${r.user}`}>
                <td><Wallet address={r.user} /></td><td className="r">{usd(num(r.debt_usd))}</td><td className="r">{away(r.health_factor)}</td><td><BandPill hf={r.health_factor} /></td>
                {market && <td><Chain id={r.chain_id} name={r.spoke_name || 'Unnamed'} href={marketPath(r)} /></td>}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <More total={sorted.length} shown={shown} step={step} set={setShown} />
    </>
  )
}

/** The wallets that hold the most of one token (`/v1/holders` rows). */
export function HolderTable({ rows }: { rows: Row[] }) {
  const { sorted, sort, toggle } = useSort(rows, { dep: (r) => num(r.supplied_usd), bor: (r) => num(r.debt_usd), markets: (r) => num(r.markets) }, { key: 'dep', dir: 'descending' })
  const th = { sort, toggle }
  return (
    <div className="scroll">
      <table>
        <thead><tr><th scope="col">Wallet</th><SortTh k="dep" r {...th}>Deposited</SortTh><SortTh k="bor" r {...th}>Borrowed</SortTh><SortTh k="markets" r {...th}>Markets</SortTh></tr></thead>
        <tbody>
          {sorted.map((r) => (
            <tr key={r.user}><td><Wallet address={r.user} /></td><td className="r">{usd(num(r.supplied_usd))}</td><td className={`r${num(r.debt_usd) ? '' : ' muted'}`}>{num(r.debt_usd) ? usd(num(r.debt_usd)) : 'nothing'}</td><td className="r">{r.markets}</td></tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
