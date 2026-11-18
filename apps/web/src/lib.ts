import { useMemo, useState } from 'react'
import type { Row } from './api.ts'

export const CHAIN_NAMES: Record<number, string> = { 1: 'Ethereum', 8453: 'Base', 43114: 'Avalanche', 5042: 'Arc' }
export const chainName = (id: number | string) => CHAIN_NAMES[Number(id)] ?? `Chain ${id}`

export type Sort = { key: string; dir: 'ascending' | 'descending' }

/** Sorts rows by a column. Numeric strings (wide integers) compare as numbers. */
export function useSort(rows: Row[], initial: Sort) {
  const [sort, setSort] = useState(initial)
  const sorted = useMemo(() => {
    const val = (r: Row) => { const v = r[sort.key]; const n = typeof v === 'string' && v !== '' ? Number(v) : v; return typeof n === 'number' && !Number.isNaN(n) ? n : v ?? Infinity }
    return [...rows].sort((a, b) => { const x = val(a), y = val(b); return (x > y ? 1 : x < y ? -1 : 0) * (sort.dir === 'ascending' ? 1 : -1) })
  }, [rows, sort])
  const toggle = (key: string) => setSort((s) => ({ key, dir: s.key === key && s.dir === 'ascending' ? 'descending' : 'ascending' }))
  return { sorted, sort, toggle }
}
