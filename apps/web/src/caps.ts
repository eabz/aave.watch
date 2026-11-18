import type { Row } from './api.ts'
import { amount, toNumber } from './format.ts'

// uint40 max: the contract's way of saying "no cap"
const NO_CAP = 1099511627775

// shortcut: caps are read as whole tokens (uint40, scaled by the asset's decimals) and usage as
// supplied / add cap and owed / draw cap; confirm the unit against Hub.sol when real rows exist.
// The raw cap is shown next to the percentage so a wrong assumption is visible.
export function capUse(r: Row): { add: number | null; draw: number | null } {
  const d = Number(r.decimals ?? 18)
  const ratio = (assets: number, cap: unknown) => { const c = Number(cap); return !c || c >= NO_CAP ? null : assets / c }
  return {
    add: ratio(toNumber(r.added_assets, d), r.add_cap),
    draw: ratio(toNumber(r.drawn_owed, d) + toNumber(r.premium_owed, d), r.draw_cap),
  }
}
export const capText = (cap: unknown) => (Number(cap) >= NO_CAP ? 'No cap' : Number(cap) === 0 ? 'Closed' : amount(String(cap), 0))
