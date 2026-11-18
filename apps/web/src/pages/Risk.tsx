import { useState } from 'react'
import { useApi } from '../api.ts'
import { capText, capUse } from '../caps.ts'
import { band, type Band, BAND_LABEL, pct, usd } from '../format.ts'
import { withChain } from '../router.ts'
import { chainName, useSort } from '../lib.ts'
import { Address, HfBadge, HfFigure, Panel, Region, SortTh, Stamp } from '../ui.tsx'

const BANDS: Band[] = ['liq', 'risk', 'watch']

function Figures({ chain }: { chain: number }) {
  const q = useApi('/v1/summary', { chain }, 15_000)
  const rows = q.data?.data
  const sum = (k: string) => rows?.reduce((a, r) => a + Number(r[k] ?? 0), 0)
  const figures: [string, string][] = [
    ['Positions below HF 1.1', rows ? String(sum('positions_below_1_1')) : '–'],
    ['Debt below HF 1.1', rows ? usd(sum('debt_below_1_1')) : '–'],
    ['Liquidatable positions', rows ? String(sum('liquidatable')) : '–'],
  ]
  return (
    <section className="panel" aria-label="Risk figures">
      <div className="stats">
        {figures.map(([label, value]) => (
          <div key={label}>
            <span className="t-label">{label}</span>
            <span className="t-display">{value}</span>
            <Stamp rows={rows} note={!rows ? (q.loading ? 'Loading.' : `Could not load: ${q.error ?? 'no answer'}.`) : rows.length === 0 ? 'No readings yet.' : undefined} />
          </div>
        ))}
      </div>
    </section>
  )
}

function Positions({ chain }: { chain: number }) {
  const q = useApi('/v1/positions', { chain, max_hf: 1.5, limit: 500 }, 15_000)
  const all = q.data?.data ?? []
  const [filter, setFilter] = useState<Band | 'all'>('all')
  const { sorted, sort, toggle } = useSort(filter === 'all' ? all : all.filter((r) => band(r.health_factor) === filter), { key: 'health_factor', dir: 'ascending' })
  const count = (b: Band) => all.filter((r) => band(r.health_factor) === b).length
  const where = chain ? `on ${chainName(chain)}` : 'on any chain'
  const tools = (
    <div className="chips" role="group" aria-label="Filter by health">
      <button className="chip" type="button" aria-pressed={filter === 'all'} onClick={() => setFilter('all')}>All <span className="n">{all.length}</span></button>
      {BANDS.map((b) => (
        <button key={b} className="chip" type="button" aria-pressed={filter === b} onClick={() => setFilter(b)}>{BAND_LABEL[b]} <span className="n">{count(b)}</span></button>
      ))}
    </div>
  )
  const source = q.data?.source === 'tip' ? 'Live, from the chain tip.' : q.data?.source === 'settled' ? 'From settled blocks only.' : ''
  return (
    <Panel title="Positions at risk" tools={tools} foot={<><Stamp rows={all} note={`Health factor below 1.5. ${source}`} /><span className="muted">{sorted.length} of {all.length} rows</span></>}>
      <Region q={q} what="positions" empty={`No positions below HF 1.5 ${where}.`}>
        {() => sorted.length === 0 ? (
          <div className="state"><p>No positions in this band {where}.</p><button className="btn btn--quiet btn--sm" type="button" onClick={() => setFilter('all')}>Show all bands</button></div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th scope="col">Account</th>
                {!chain && <SortTh label="Chain" k="chain_id" sort={sort} toggle={toggle} />}
                <SortTh label="Spoke" k="spoke_name" sort={sort} toggle={toggle} />
                <SortTh label="Collateral, USD" k="collateral_usd" sort={sort} toggle={toggle} num />
                <SortTh label="Debt, USD" k="debt_usd" sort={sort} toggle={toggle} num />
                <SortTh label="Risk premium" k="risk_premium" sort={sort} toggle={toggle} num />
                <SortTh label="Health factor" k="health_factor" sort={sort} toggle={toggle} num />
                <th scope="col">Status</th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((r) => (
                <tr key={`${r.chain_id}:${r.spoke}:${r.user}`}>
                  <td><Address value={r.user} label="account" href={withChain(`/account/${r.user}`, r.chain_id)} /></td>
                  {!chain && <td>{chainName(r.chain_id)}</td>}
                  <td>{r.spoke_name || <Address value={r.spoke} label="Spoke" />}</td>
                  <td className="num">{usd(r.collateral_usd)}</td>
                  <td className="num">{usd(r.debt_usd)}</td>
                  <td className="num">{pct(r.risk_premium, 2)}</td>
                  <td className="num"><HfFigure value={r.health_factor} /></td>
                  <td><HfBadge value={r.health_factor} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Region>
    </Panel>
  )
}

function CapPressure({ chain }: { chain: number }) {
  const q = useApi('/v1/hub-spokes', { chain }, 30_000)
  const hot = (q.data?.data ?? []).map((r) => ({ ...r, ...capUse(r) })).filter((r) => (r.add ?? 0) >= 0.9 || (r.draw ?? 0) >= 0.9)
  const view = { ...q, data: q.data ? { data: hot } : null }
  const tone = (v: number | null) => (v != null && v >= 0.9 ? { color: 'var(--danger-text)' } : undefined)
  return (
    <Panel title="Cap pressure" foot={<Stamp rows={hot} note="Lines at or above 90% of their add cap or draw cap." />}>
      <Region q={view} what="cap usage" empty={`No Spoke is at or above 90% of a cap ${chain ? `on ${chainName(chain)}` : 'on any chain'}.`}>
        {(rows) => (
          <table className="table">
            <thead>
              <tr>
                <th scope="col">Hub asset</th><th scope="col">Spoke</th>
                <th scope="col" className="num">Add cap used</th><th scope="col" className="num">Add cap, tokens</th>
                <th scope="col" className="num">Draw cap used</th><th scope="col" className="num">Draw cap, tokens</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={`${r.chain_id}:${r.hub}:${r.asset_id}:${r.spoke}`}>
                  <td>{r.symbol || `Asset ${r.asset_id}`}{!chain && <span className="muted"> on {chainName(r.chain_id)}</span>}</td>
                  <td>{r.spoke_name || <Address value={r.spoke} label="Spoke" />}</td>
                  <td className="num" style={tone(r.add)}>{pct(r.add)}</td><td className="num">{capText(r.add_cap)}</td>
                  <td className="num" style={tone(r.draw)}>{pct(r.draw)}</td><td className="num">{capText(r.draw_cap)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Region>
    </Panel>
  )
}

export function Risk({ chain }: { chain: number }) {
  return (
    <div className="page">
      <header><h1 className="t-h1">Risk</h1><p className="muted">Positions closest to liquidation and Spokes closest to their caps{chain ? `, on ${chainName(chain)}` : ', on every chain'}.</p></header>
      <Figures chain={chain} />
      <Positions chain={chain} />
      <CapPressure chain={chain} />
    </div>
  )
}
