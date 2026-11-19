import { useEffect, useRef } from 'react'
import { type Envelope, useStream } from './api.ts'

// One connection to /v1/stream for the whole app; pages listen to the topics they care about.
type Listener = (e: Envelope) => void
const listeners: Record<string, Set<Listener>> = { feed: new Set(), risk: new Set() }

/** Opens the stream. Called once, by the app shell. While it is connected the dot on the mark pulses. */
export function useLive() {
  const status = useStream({}, ['feed', 'risk'], (topic, e) => listeners[topic]?.forEach((l) => l(e)))
  useEffect(() => { document.documentElement.toggleAttribute('data-live', status === 'live') }, [status])
}

/** Calls `on` for every message of a topic while the component is shown. */
export function useTopic(topic: 'feed' | 'risk', on: Listener) {
  const handler = useRef(on)
  useEffect(() => { handler.current = on })
  useEffect(() => {
    const l: Listener = (e) => handler.current(e)
    listeners[topic].add(l)
    return () => { listeners[topic].delete(l) }
  }, [topic])
}

/** Runs `run` soon after the first call and at most once per `wait` ms: many live messages, one refetch. */
export function useThrottled(run: () => void, wait = 5000) {
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const fn = useRef(run)
  useEffect(() => { fn.current = run })
  useEffect(() => () => clearTimeout(timer.current), [])
  return () => { timer.current ??= setTimeout(() => { timer.current = undefined; fn.current() }, wait) }
}
