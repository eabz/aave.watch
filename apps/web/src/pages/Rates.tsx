import { useMemo, useState } from 'react'
import { type Row, useApi } from '../api.ts'
import { short, usd } from '../format.ts'
import { CHAINS, chainName, hubKey, type Kind, kind, KIND_LABEL, KINDS, num, sum } from '../lib.ts'
import { AssetTable } from '../tables.tsx'
import { poolPath } from '../lib.ts'
import { Card, Chain, Region, Stamp, Used } from '../ui.tsx'

export function Rates() {
  const assets = useApi('/v1/hub-assets', {}, 30_000)
  const [k, setK] = useState<Kind | ''>('')
  const [chain, setChain] = useState(0)
  const all = assets.data?.data
  const rows = useMemo(() => (all ?? []).filter((r) => (!k || kind(r.symbol) === k) && (!chain || hubKey(r).chain === chain)).sort((a, b) => num(b.added_usd) - num(a.added_usd)), [all, k, chain])
  const pools = useMemo(() => {
    const by = new Map<string, Row[]>()
    for (const r of all ?? []) { const key = poolPath(r); by.set(key, [...(by.get(key) ?? []), r]) }
    return [...by.values()].map((rs) => ({ first: rs[0], dep: sum(rs, 'added_usd'), bor: sum(rs, 'owed_usd'), n: rs.filter((r) => num(r.added_usd) > 0).length })).filter((p) => p.dep > 0).sort((a, b) => b.dep - a.dep)
  }, [all])

  return (
    <>
      <h1 className="visually-hidden">Rates</h1>
      <Card title="Assets" tools={
        <span className="pills">
          <span className="seg" role="group" aria-label="Kind of asset">
            <button type="button" aria-pressed={k === ''} onClick={() => setK('')}>All</button>
            {KINDS.map((x) => <button key={x} type="button" aria-pressed={k === x} onClick={() => setK(x)}>{KIND_LABEL[x]}</button>)}
          </span>
          <span className="seg" role="group" aria-label="Chain">
            <button type="button" aria-pressed={chain === 0} onClick={() => setChain(0)}>All chains</button>
            {CHAINS.map((c) => <button key={c} type="button" aria-pressed={chain === c} onClick={() => setChain(c)}>{chainName(c)}</button>)}
          </span>
        </span>
      } note="Earn is measured over the last 7 days. Borrow is the rate right now. From 90% lent out, taking money out can be slow.">
        <Region q={assets} what="the rates" empty="No assets yet." hold={1300}>
          {() => rows.length === 0 ? <span className="muted">Nothing matches these two filters.</span> : (
<AssetTable key={`${k}${chain}`} rows={rows} />
          )}
        </Region>
      </Card>
      <Card title="Pools" note="A pool holds the deposits that markets lend out.">
        <Region q={assets} what="the pools" empty="No pools yet.">
          {() => (
            <div className="scroll"><table>
              <thead><tr><th scope="col">Pool</th><th scope="col" className="r">Deposited</th><th scope="col" className="r">Borrowed</th><th scope="col" className="r">Assets</th><th scope="col" className="r">Lent out</th></tr></thead>
              <tbody>{pools.map((p) => (
                <tr key={poolPath(p.first)}>
                  <td><Chain id={hubKey(p.first).chain} name={p.first.hub_name || short(hubKey(p.first).hub)} href={poolPath(p.first)} /></td>
                  <td className="r">{usd(p.dep)}</td><td className="r">{usd(p.bor)}</td><td className="r">{p.n}</td><td><Used value={p.dep ? p.bor / p.dep : 0} /></td>
                </tr>
              ))}</tbody>
            </table></div>
          )}
        </Region>
      </Card>
      <Stamp rows={(all ?? []).slice(0, 5)} />
    </>
  )
}
