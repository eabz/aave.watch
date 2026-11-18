import { useSyncExternalStore } from 'react'

// A path router small enough to read: pushState, popstate, and one click handler for same-origin links.
const listeners = new Set<() => void>()
const emit = () => listeners.forEach((l) => l())
const subscribe = (l: () => void) => {
  listeners.add(l)
  window.addEventListener('popstate', l)
  return () => { listeners.delete(l); window.removeEventListener('popstate', l) }
}

export function navigate(to: string, replace = false) {
  if (to === location.pathname + location.search) return
  history[replace ? 'replaceState' : 'pushState'](null, '', to)
  emit()
}

export function useLocation() {
  const href = useSyncExternalStore(subscribe, () => location.pathname + location.search)
  const [path, query = ''] = href.split('?')
  return { path, query: new URLSearchParams(query) }
}

/** The chain filter lives in the URL (?chain=1). 0 means all chains. */
export function useChain(): number {
  return Number(useLocation().query.get('chain')) || 0
}

/** Keeps the chain filter when moving between pages. */
export const withChain = (path: string, chain: number) => (chain ? `${path}?chain=${chain}` : path)

document.addEventListener('click', (e) => {
  const a = (e.target as Element).closest?.('a')
  if (!a || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || a.target || a.origin !== location.origin || a.hash) return
  e.preventDefault()
  navigate(a.pathname + a.search)
})
