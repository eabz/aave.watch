import { useCallback, useState } from 'react'
import { type Envelope, type Row, useApi, useStream } from '../api.ts'
import { age, block, usd } from '../format.ts'
import { withChain } from '../router.ts'
import { chainName } from '../lib.ts'
import { Address, Icon, Panel, Region } from '../ui.tsx'

const MAX = 200
const id = (r: Row) => `${r.chain_id}:${r.block_number}:${r.log_index}`

export function EventList({ rows, live, chain }: { rows: Row[]; live?: Set<string>; chain: number }) {
  return (
    <ul className="feed">
      {rows.map((r) => (
        <li key={id(r)} className={live?.has(id(r)) ? 'is-changed' : undefined}>
          <span className="t-caption" title={`Block ${block(r.block_number)}${r.block_time ? `, ${r.block_time} UTC` : ''}`}>{r.block_time ? age(r.block_time) : `Block ${block(r.block_number)}`}</span>
          <span><span className="badge badge--code">{r.name}</span></span>
          <span className="what">
            {[r.spoke_name || (r.kind === 'hub' ? 'Hub' : ''), r.symbol, !chain && chainName(r.chain_id)].filter(Boolean).join(', ')}
            {r.user ? <>, by <Address value={r.user} label="account" href={withChain(`/account/${r.user}`, r.chain_id)} /></> : null}
            {r.settled === false ? <span className="muted"> (not settled)</span> : null}
          </span>
          <span className="num">{Number(r.amount_usd) ? usd(Number(r.amount_usd)) : <span className="muted">–</span>}</span>
          <span className="tx">{r.tx_hash ? <Address value={r.tx_hash} label="transaction" /> : null}</span>
        </li>
      ))}
    </ul>
  )
}

export function Feed({ chain }: { chain: number }) {
  const first = useApi('/v1/events', { chain, limit: 100 })
  const [live, setLive] = useState<Row[]>([])

  const onMessage = useCallback((topic: string, e: Envelope) => {
    if (topic === 'chain') {
      // a reorg replaces unsettled blocks: drop what we showed from them
      const from = e.from_block ?? e.data?.from_block
      if (e.type === 'reorg' && from != null) setLive((l) => l.filter((r) => r.settled || r.chain_id !== e.chain_id || r.block_number < from))
      return
    }
    const row: Row = { ...e.data, chain_id: e.chain_id, block_number: e.block, log_index: e.log_index, name: e.name ?? e.data?.name, settled: e.settled }
    setLive((l) => (l.some((r) => id(r) === id(row)) ? l : [row, ...l].slice(0, MAX)))
  }, [])
  const status = useStream({ chain }, ['feed', 'chain'], onMessage)

  const seen = new Set(live.map(id))
  const older = (first.data?.data ?? []).filter((r) => !seen.has(id(r)))
  const rows = [...live.filter((r) => !chain || r.chain_id === chain), ...older].slice(0, MAX)
  const view = { ...first, data: first.data || live.length ? { data: rows } : null }

  return (
    <div className="page">
      <header><h1 className="t-h1">Live feed</h1><p className="muted">Every decoded event as it lands{chain ? ` on ${chainName(chain)}` : ''}: when, what, where, how much, and the transaction.</p></header>
      {status === 'down' && (
        <div className="banner banner--warning" role="status">
          <Icon name="alert" />
          <p>The live stream is not connected. The list shows events up to the last settled block; the browser keeps trying to reconnect.</p>
        </div>
      )}
      <Panel title="Events" foot={<><span className="t-caption">{status === 'live' ? 'Stream connected. New events appear at the top.' : status === 'connecting' ? 'Connecting to the stream.' : 'Stream not connected.'}</span><span className="muted">{rows.length} rows, newest first</span></>}>
        <Region q={view} what="events" empty={`No events ${chain ? `on ${chainName(chain)}` : 'on any chain'} yet.`}>
          {(list) => <EventList rows={list} live={seen} chain={chain} />}
        </Region>
      </Panel>
    </div>
  )
}
