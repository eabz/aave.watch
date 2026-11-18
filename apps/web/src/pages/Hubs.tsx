import { useState } from 'react'
import { type Row, useApi } from '../api.ts'
import { amount, pct, rayAmount, usd } from '../format.ts'
import { capText, capUse } from '../caps.ts'
import { chainName } from '../lib.ts'
import { Address, Panel, Region, Stamp } from '../ui.tsx'

const key = (r: Row) => `${r.chain_id}:${r.hub ?? r.address}${r.asset_id === undefined ? '' : `:${r.asset_id}`}`

function Meter({ value, label }: { value: number | null; label: string }) {
  if (value == null) return <span className="muted">{label}</span>
  return (
    <span style={{ display: 'inline-grid', gridTemplateColumns: '6rem max-content', gap: 'var(--s-2)', alignItems: 'center' }}>
      <span className={`meter${value >= 0.9 ? ' meter--over' : ''}`} role="img" aria-label={`${pct(value)} of cap used`}><i style={{ '--v': Math.min(1, value) } as React.CSSProperties} /></span>
      <span style={value >= 0.9 ? { color: 'var(--danger-text)' } : undefined}>{pct(value)}</span>
    </span>
  )
}

function Allocation({ asset }: { asset: Row }) {
  const q = useApi('/v1/hub-spokes', { chain: asset.chain_id, hub: asset.hub, asset_id: String(asset.asset_id) }, 30_000)
  const name = `${asset.symbol || `Asset ${asset.asset_id}`}, ${asset.hub_name || 'Hub'}`
  return (
    <Panel title={`Spoke allocation: ${name}`} foot={<Stamp rows={q.data?.data} note="Caps are in whole tokens." />}>
      <Region q={q} what="the Spoke allocation" empty={`No Spoke is connected to ${name} yet.`}>
        {(rows) => (
          <table className="table">
            <thead>
              <tr>
                <th scope="col">Spoke</th><th scope="col">Kind</th>
                <th scope="col" className="num">Supplied</th><th scope="col" className="num">Supplied, USD</th><th scope="col">Add cap used</th><th scope="col" className="num">Add cap</th>
                <th scope="col" className="num">Owed</th><th scope="col" className="num">Owed, USD</th><th scope="col">Draw cap used</th><th scope="col" className="num">Draw cap</th>
                <th scope="col">State</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const d = Number(r.decimals ?? 18), use = capUse(r)
                return (
                  <tr key={r.spoke}>
                    <td>{r.spoke_name || <Address value={r.spoke} label="Spoke" />}</td>
                    <td className="muted">{r.spoke_kind || '–'}</td>
                    <td className="num">{amount(r.added_assets, d)}</td><td className="num">{usd(r.added_usd)}</td>
                    <td><Meter value={use.add} label={capText(r.add_cap)} /></td><td className="num">{capText(r.add_cap)}</td>
                    <td className="num">{amount((BigInt(r.drawn_owed ?? 0) + BigInt(r.premium_owed ?? 0)).toString(), d)}</td><td className="num">{usd(r.owed_usd)}</td>
                    <td><Meter value={use.draw} label={capText(r.draw_cap)} /></td><td className="num">{capText(r.draw_cap)}</td>
                    <td>{Number(r.halted) ? <span className="badge badge--risk">Halted</span> : Number(r.active) ? <span className="badge">Active</span> : <span className="badge badge--watch">Inactive</span>}</td>
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

export function Hubs({ chain }: { chain: number }) {
  const hubs = useApi('/v1/hubs', { chain })
  const [hub, setHub] = useState('')
  const [picked, setPicked] = useState('')
  const list = hubs.data?.data ?? []
  const current = list.find((h) => key(h) === hub) ?? list[0]
  const assets = useApi(current ? '/v1/hub-assets' : null, { chain: current?.chain_id, hub: current?.address }, 30_000)
  const asset = assets.data?.data.find((a) => key(a) === picked)
  const where = chain ? `on ${chainName(chain)}` : 'on any chain'

  return (
    <div className="page">
      <header><h1 className="t-h1">Hubs</h1><p className="muted">What each Hub holds, what is drawn from it, and which Spokes draw it.</p></header>
      <section className="panel" aria-label="Hubs">
        <div className="panel__body">
          <Region q={hubs} what="the Hub list" empty={`No Hub is indexed ${where} yet.`}>
            {(rows) => (
              <div className="chips" role="group" aria-label="Hub">
                {rows.map((h) => (
                  <button key={key(h)} className="chip" type="button" aria-pressed={current !== undefined && key(h) === key(current)} onClick={() => { setHub(key(h)); setPicked('') }}>
                    {h.name || <span className="mono">{h.address.slice(0, 8)}</span>}{!chain && <span className="n">{chainName(h.chain_id)}</span>}
                  </button>
                ))}
              </div>
            )}
          </Region>
        </div>
      </section>
      {current && (
        <Panel title={`${current.name || 'Hub'} assets, ${chainName(current.chain_id)}`} foot={<Stamp rows={assets.data?.data} note="Select an asset to see its Spokes." />}>
          <Region q={assets} what="the Hub assets" empty={`No asset readings for ${current.name || 'this Hub'} yet.`}>
            {(rows) => (
              <table className="table">
                <thead>
                  <tr>
                    <th scope="col">Asset</th>
                    <th scope="col" className="num">Supplied</th><th scope="col" className="num">Supplied, USD</th>
                    <th scope="col" className="num">Drawn</th><th scope="col" className="num">Premium owed</th><th scope="col" className="num">Owed, USD</th>
                    <th scope="col" className="num">Idle</th><th scope="col" className="num">Idle, USD</th>
                    <th scope="col" className="num">Usage</th><th scope="col" className="num">Base rate</th><th scope="col" className="num">Deficit</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => {
                    const d = Number(r.decimals ?? 18), deficit = rayAmount(r.deficit_ray, d)
                    return (
                      <tr key={key(r)} aria-selected={key(r) === picked}>
                        <td><button className="row-btn" type="button" aria-expanded={key(r) === picked} onClick={() => setPicked(key(r) === picked ? '' : key(r))}>{r.symbol || `Asset ${r.asset_id}`}</button></td>
                        <td className="num">{amount(r.added_assets, d)}</td><td className="num">{usd(r.added_usd)}</td>
                        <td className="num">{amount(r.drawn_owed, d)}</td><td className="num">{amount(r.premium_owed, d)}</td><td className="num">{usd(r.owed_usd)}</td>
                        <td className="num">{amount(r.liquidity, d)}</td><td className="num">{usd(r.liquidity_usd)}</td>
                        <td className="num">{pct(r.usage)}</td><td className="num">{pct(r.drawn_rate_apr, 2)}</td>
                        <td className="num" style={deficit !== '0.00' && deficit !== '–' ? { color: 'var(--danger-text)' } : undefined}>{deficit}</td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            )}
          </Region>
        </Panel>
      )}
      {asset && <Allocation asset={asset} />}
    </div>
  )
}
