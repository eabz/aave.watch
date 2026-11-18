import { useCallback, useEffect, useRef, useState } from 'react'

// Rows keep the ClickHouse column names (docs/api.md). Wide integers arrive as strings.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Row = Record<string, any>
export type Params = Record<string, string | number | null | undefined | false>
export type Query<T> = { data: T | null; error: string | null; loading: boolean; refetch: () => void }

export function url(path: string, params: Params = {}): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) if (v !== null && v !== undefined && v !== false && v !== '' && v !== 0) q.set(k, String(v))
  const s = q.toString()
  return s ? `${path}?${s}` : path
}

/** Fetches a list endpoint. `path: null` skips the request. `interval` refetches (ms) without clearing the rows. */
export function useApi<T = { data: Row[]; source?: string }>(path: string | null, params: Params = {}, interval = 0): Query<T> {
  const target = path === null ? null : url(path, params)
  const [state, setState] = useState<{ target: string | null; data: T | null; error: string | null }>({ target: null, data: null, error: null })
  const [tick, setTick] = useState(0)
  const refetch = useCallback(() => setTick((t) => t + 1), [])

  useEffect(() => {
    if (target === null) return
    const ctl = new AbortController()
    fetch(target, { signal: ctl.signal, headers: { accept: 'application/json' } })
      .then(async (r) => {
        const body = await r.json().catch(() => null)
        if (!r.ok) throw new Error(body?.error ?? `The API answered ${r.status}`)
        if (body === null) throw new Error('The API did not answer with JSON')
        setState({ target, data: body as T, error: null })
      })
      .catch((e: Error) => {
        // keep the last good rows on a failed refetch; the stamp shows how old they are
        if (e.name !== 'AbortError') setState((s) => ({ target, data: s.target === target ? s.data : null, error: e.message }))
      })
    const timer = interval ? setInterval(refetch, interval) : undefined
    return () => { ctl.abort(); clearInterval(timer) }
  }, [target, tick, interval, refetch])

  const current = state.target === target
  return { data: current ? state.data : null, error: current ? state.error : null, loading: target !== null && !current, refetch }
}

export type Envelope = { v: number; chain_id: number; block: number; block_hash?: string; log_index?: number; settled?: boolean; type: string; name?: string; from_block?: number; data?: Row }
export type StreamStatus = 'connecting' | 'live' | 'down'

/** Server-Sent Events from /v1/stream. The browser reconnects by itself and resumes with Last-Event-ID. */
export function useStream(params: Params, topics: string[], onMessage: (topic: string, e: Envelope) => void): StreamStatus {
  const [state, setStatus] = useState<{ target: string; status: StreamStatus }>({ target: '', status: 'connecting' })
  const handler = useRef(onMessage)
  useEffect(() => { handler.current = onMessage })
  const target = url('/v1/stream', { ...params, topics: topics.join(',') })

  useEffect(() => {
    const es = new EventSource(target)
    es.onopen = () => setStatus({ target, status: 'live' })
    es.onerror = () => setStatus({ target, status: 'down' })
    const listen = (topic: string) => (m: MessageEvent<string>) => {
      try { handler.current(topic, JSON.parse(m.data) as Envelope) } catch { /* one malformed message must not stop the feed */ }
    }
    const names = target.match(/topics=([^&]*)/)?.[1] ?? ''
    for (const t of decodeURIComponent(names).split(',')) es.addEventListener(t, listen(t))
    return () => es.close()
  }, [target])

  return state.target === target ? state.status : 'connecting'
}
