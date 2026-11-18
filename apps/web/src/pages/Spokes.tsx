import { useState } from 'react'
import { type Row, useApi } from '../api.ts'
import { amount, bps, price, usd } from '../format.ts'
import { chainName } from '../lib.ts'
import { Address, Panel, Region, Stamp } from '../ui.tsx'

const KINDS: [string, string, string][] = [
  ['spoke', 'Standard Spokes', 'Lending Spokes with positions, health factors and liquidations.'],
  ['tokenization', 'Tokenization Spokes', 'ERC-4626 vaults over one Hub asset. No positions or health factors.'],
  ['treasury', 'Treasury Spokes', 'Receive protocol fees.'],
]
const key = (s: Row) => `${s.chain_id}:${s.address}`

function Reserves({ spoke }: { spoke: Row }) {
  const q = useApi('/v1/reserves', { chain: spoke.chain_id, spoke: spoke.address }, 30_000)
  const name = spoke.name || 'this Spoke'
  return (
    <Panel title={`Reserves: ${spoke.name || spoke.address}, ${chainName(spoke.chain_id)}`} foot={<Stamp rows={q.data?.data} />}>
      <Region q={q} what="the reserves" empty={`No reserve readings for ${name} yet.`}>
        {(rows) => (
          <table className="table">
            <thead>
              <tr>
                <th scope="col">Reserve</th>
                <th scope="col" className="num">Supplied</th><th scope="col" className="num">Supplied, USD</th>
                <th scope="col" className="num">Debt</th><th scope="col" className="num">Debt, USD</th>
                <th scope="col" className="num">Price, USD</th><th scope="col" className="num">Collateral factor</th><th scope="col" className="num">Collateral risk</th>
                <th scope="col" className="num">Liquidation fee</th><th scope="col">State</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const d = Number(r.decimals ?? 18)
                const flags = [Number(r.paused) && ['risk', 'Paused'], Number(r.frozen) && ['watch', 'Frozen'], !Number(r.borrowable) && ['info', 'Not borrowable']].filter(Boolean) as [string, string][]
                return (
                  <tr key={r.reserve_id}>
                    <td>{r.symbol || `Reserve ${r.reserve_id}`}</td>
                    <td className="num">{amount(r.supplied_assets, d)}</td><td className="num">{usd(r.supplied_usd)}</td>
                    <td className="num">{amount((BigInt(r.drawn_debt ?? 0) + BigInt(r.premium_debt ?? 0)).toString(), d)}</td><td className="num">{usd(r.debt_usd)}</td>
                    <td className="num">{price(r.price)}</td><td className="num">{bps(r.collateral_factor)}</td><td className="num">{bps(r.collateral_risk)}</td>
                    <td className="num">{bps(r.liquidation_fee)}</td>
                    <td><span style={{ display: 'inline-flex', gap: 'var(--s-1)' }}>{flags.length ? flags.map(([tone, label]) => <span key={label} className={`badge badge--${tone}`}>{label}</span>) : <span className="badge">Active</span>}</span></td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </Region>
    </Panel>
  )
}

export function Spokes({ chain }: { chain: number }) {
  const q = useApi('/v1/spokes', { chain })
  const [picked, setPicked] = useState('')
  const all = q.data?.data ?? []
  const spoke = all.find((s) => key(s) === picked)
  const known = new Set(KINDS.map(([k]) => k))
  const groups = [...KINDS, ...(all.some((s) => !known.has(s.kind)) ? [['', 'Other Spokes', 'Kinds this page does not know yet.'] as [string, string, string]] : [])]

  return (
    <div className="page">
      <header><h1 className="t-h1">Spokes</h1><p className="muted">Every Spoke connected to a Hub, by kind. Select a standard Spoke to see its reserves.</p></header>
      {!q.data ? (
        <section className="panel" aria-label="Spokes"><Region q={q} what="the Spoke list" empty="">{() => null}</Region></section>
      ) : all.length === 0 ? (
        <section className="panel" aria-label="Spokes"><div className="state"><p>No Spoke is indexed {chain ? `on ${chainName(chain)}` : 'on any chain'} yet.</p></div></section>
      ) : (
        <div className="groups">
          {groups.map(([kind, title, about]) => {
            const rows = all.filter((s) => (kind ? s.kind === kind : !known.has(s.kind)))
            if (!rows.length) return null
            return (
              <Panel key={kind} title={`${title} (${rows.length})`} foot={<span className="t-caption">{about}</span>}>
                <table className="table">
                  <thead><tr><th scope="col">Spoke</th>{!chain && <th scope="col">Chain</th>}<th scope="col">Address</th><th scope="col">Oracle</th><th scope="col" className="num">First block</th></tr></thead>
                  <tbody>
                    {rows.map((s) => (
                      <tr key={key(s)} aria-selected={key(s) === picked}>
                        <td>{kind === 'spoke'
                          ? <button className="row-btn" type="button" aria-expanded={key(s) === picked} onClick={() => setPicked(key(s) === picked ? '' : key(s))}>{s.name || 'Unnamed Spoke'}</button>
                          : s.name || 'Unnamed Spoke'}</td>
                        {!chain && <td>{chainName(s.chain_id)}</td>}
                        <td><Address value={s.address} label="Spoke" /></td>
                        <td>{s.oracle ? <Address value={s.oracle} label="oracle" /> : <span className="muted">–</span>}</td>
                        <td className="num">{Number(s.first_block) ? Number(s.first_block).toLocaleString('en-US') : '–'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Panel>
            )
          })}
        </div>
      )}
      {spoke && <Reserves spoke={spoke} />}
    </div>
  )
}
