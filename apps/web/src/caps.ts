import type { Row } from './api.ts'
import { toNumber } from './format.ts'

// uint40 max: the contract's way of saying "no limit"
const NO_CAP = 1099511627775

// shortcut: limits are read as whole tokens (uint40, scaled by the asset's decimals) and use as
// borrowed / draw limit and deposited / add limit; confirm the unit against Hub.sol.
/** How much of its limits one market has used on one pool asset, as fractions. `null` means no limit. */
export function limitUse(r: Row): { add: number | null; draw: number | null; drawCap: number | null } {
  const d = Number(r.decimals ?? 18)
  const cap = (v: unknown) => { const c = Number(v); return !c || c >= NO_CAP ? null : c }
  const add = cap(r.add_cap), draw = cap(r.draw_cap)
  return {
    add: add && toNumber(r.added_assets, d) / add,
    draw: draw && (toNumber(r.drawn_owed, d) + toNumber(r.premium_owed, d)) / draw,
    drawCap: draw,
  }
}
