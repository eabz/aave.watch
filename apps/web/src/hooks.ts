import { useEffect, useState } from 'react'

/** Re-renders on a timer so "4s ago" keeps counting. */
export function useNow(every = 5000) {
  const [now, setNow] = useState(Date.now)
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), every); return () => clearInterval(t) }, [every])
  return now
}

/** Whether "Show blocks" is on. It is a page-wide choice, remembered (index.html restores it before the first paint). */
export function useBlocks(): [boolean, () => void] {
  const [on, setOn] = useState(() => document.documentElement.hasAttribute('data-blocks'))
  const toggle = () => {
    const next = document.documentElement.toggleAttribute('data-blocks')
    try { localStorage.setItem('aw-blocks', next ? '1' : '') } catch { /* private mode: the choice lasts for this page */ }
    setOn(next)
  }
  return [on, toggle]
}

export type Sort = { key: string; dir: 'ascending' | 'descending' }
type Getters<T> = Record<string, (row: T) => number | string>
/** Sorts rows by a column the reader picks. Numbers start biggest first, names A to Z. */
export function useSort<T>(rows: T[], get: Getters<T>, initial: Sort) {
  const [sort, setSort] = useState(initial)
  const by = get[sort.key]
  const sorted = by ? [...rows].sort((a, b) => { const x = by(a), y = by(b); return (x > y ? 1 : x < y ? -1 : 0) * (sort.dir === 'ascending' ? 1 : -1) }) : rows
  const toggle = (key: string) => setSort((s) => ({ key, dir: s.key === key ? (s.dir === 'ascending' ? 'descending' : 'ascending') : typeof get[key](rows[0]) === 'string' ? 'ascending' : 'descending' }))
  return { sorted, sort, toggle }
}
