import { type Row, useApi } from '../api.ts'
import { pct, price, usd } from '../format.ts'
import { CHAIN_NAMES, chainName } from '../lib.ts'
import { Address, HfBadge, HfFigure, Panel, Region, Stamp } from '../ui.tsx'
import { withChain } from '../router.ts'
import { EventList } from './Feed.tsx'

export function Account({ chain, user }: { chain: number; user: string }) {
  const q = useApi<{ data: Row[] }>(chain ? '/v1/account' : null, { chain, user }, 30_000)
  const events = useApi(chain ? '/v1/events' : null, { chain, user, limit: 100 })

  return (
    <div className="page">
      <header>
        <h1 className="t-h1">Account</h1>
        <p><Address value={user} label="account" /></p>
      </header>
      {!chain ? (
        <section className="panel" aria-label="Choose a chain">
          <div className="state">
            <p>An account is read one chain at a time. Choose the chain to read {`${user.slice(0, 6)}…${user.slice(-4)}`} on.</p>
            <div className="chips">
              {Object.entries(CHAIN_NAMES).map(([id, name]) => <a key={id} className="chip" href={withChain(`/account/${user}`, Number(id))}>{name}</a>)}
            </div>
          </div>
        </section>
      ) : (
        <>
          <Region q={q} what="this account" empty={`This account has no position on ${chainName(chain)}.`}>
            {(spokes) => (
              <div className="groups">
                {spokes.map((s) => (
                  <Panel key={s.spoke} title={`${s.spoke_name || 'Spoke'}, ${chainName(chain)}`} tools={<HfBadge value={s.health_factor} />} foot={<Stamp rows={[s]} />}>
                    <div className="stats">
                      <div><span className="t-label">Health factor</span><span className="t-display"><HfFigure value={s.health_factor} /></span></div>
                      <div><span className="t-label">Collateral</span><span className="t-display">{usd(s.collateral_usd)}</span></div>
                      <div><span className="t-label">Debt</span><span className="t-display">{usd(s.debt_usd)}</span><span className="t-caption">Risk premium {pct(s.risk_premium, 2)}</span></div>
                    </div>
                    {s.reserves?.length ? (
                      <table className="table">
                        <thead><tr><th scope="col">Reserve</th><th scope="col" className="num">Supplied, USD</th><th scope="col" className="num">Debt, USD</th><th scope="col" className="num">Price, USD</th><th scope="col">Use</th></tr></thead>
                        <tbody>
                          {s.reserves.map((r: Row) => (
                            <tr key={r.reserve_id}>
                              <td>{r.symbol || `Reserve ${r.reserve_id}`}</td>
                              <td className="num">{usd(r.supplied_usd)}</td><td className="num">{usd(r.debt_usd)}</td><td className="num">{price(r.price)}</td>
                              <td><span style={{ display: 'inline-flex', gap: 'var(--s-1)' }}>
                                {Number(r.is_collateral) ? <span className="badge">Collateral</span> : null}{Number(r.is_borrowing) ? <span className="badge">Borrowed</span> : null}
                              </span></td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    ) : <div className="state"><p>No reserve readings for this position yet.</p></div>}
                  </Panel>
                ))}
              </div>
            )}
          </Region>
          <Panel title="Events of this account" foot={<span className="t-caption">The 100 newest, from settled blocks.</span>}>
            <Region q={events} what="this account's events" empty={`No events for this account on ${chainName(chain)}.`}>
              {(rows) => <EventList rows={rows} chain={chain} />}
            </Region>
          </Panel>
        </>
      )}
    </div>
  )
}
