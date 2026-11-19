import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import { type Row, useApi } from './api.ts'
import { short, usd } from './format.ts'
import { hubKey, num, tokenPath } from './lib.ts'
import { CloseCalls } from './pages/CloseCalls.tsx'
import { Market } from './pages/Market.tsx'
import { Now } from './pages/Now.tsx'
import { Pool } from './pages/Pool.tsx'
import { Rates } from './pages/Rates.tsx'
import { Token as TokenPage } from './pages/Token.tsx'
import { Wallet as WalletPage } from './pages/Wallet.tsx'
import { useLive } from './live.ts'
import { navigate, useLocation } from './router.ts'
import { Chain, Icon, KindPill, Mark, Token } from './ui.tsx'

const MENU = [['/', 'Now'], ['/rates', 'Rates'], ['/close-calls', 'Close calls']] as const
const ADDRESS = /^0x[0-9a-f]{40}$/

type Hit = { key: string; href: string; group: string; body: ReactNode; note?: string; text: string; size: number }

/** One box for tokens, pools, markets and wallets (docs/branding.md, "Search"). */
function Search() {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const [sel, setSel] = useState(0)
  const input = useRef<HTMLInputElement>(null)
  // loaded the first time the box opens
  const assets = useApi(open ? '/v1/hub-assets' : null).data?.data
  const hubs = useApi(open ? '/v1/hubs' : null).data?.data
  const spokes = useApi(open ? '/v1/spokes' : null, { kind: 'spoke' }).data?.data
  const lines = useApi(open ? '/v1/hub-spokes' : null).data?.data

  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === '/' && !(e.target as HTMLElement).closest('input, select, textarea')) { e.preventDefault(); input.current?.focus() }
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  }, [])

  const all = useMemo<Hit[]>(() => {
    const hits: Hit[] = []
    const tokens = new Map<string, { row: Row; usd: number }>()
    for (const r of assets ?? []) { const k = String(r.symbol).toLowerCase(), t = tokens.get(k); if (t) t.usd += num(r.added_usd); else tokens.set(k, { row: r, usd: num(r.added_usd) }) }
    for (const [k, t] of tokens) hits.push({ key: `t${k}`, href: tokenPath(k), group: 'Tokens', text: k, size: t.usd, note: `${usd(t.usd)} deposited`,
      body: <><Token chain={hubKey(t.row).chain} token={t.row.token} symbol={t.row.symbol} plain /><KindPill symbol={t.row.symbol} /></> })
    for (const h of hubs ?? []) {
      const total = (assets ?? []).filter((r) => hubKey(r).hub === h.address && hubKey(r).chain === Number(h.chain_id)).reduce((a, r) => a + num(r.added_usd), 0)
      hits.push({ key: `p${h.chain_id}${h.address}`, href: `/pool/${h.chain_id}/${h.address}`, group: 'Pools', text: `${h.name} pool`.toLowerCase(), size: total, note: `${usd(total)} deposited`, body: <Chain id={h.chain_id} name={h.name} /> })
    }
    for (const s of spokes ?? []) {
      const owed = (lines ?? []).filter((r) => r.spoke === s.address && Number(r.chain_id) === Number(s.chain_id)).reduce((a, r) => a + num(r.owed_usd), 0)
      hits.push({ key: `m${s.chain_id}${s.address}`, href: `/market/${s.chain_id}/${s.address}`, group: 'Markets', text: `${s.name || s.address} market`.toLowerCase(), size: owed, note: `${usd(owed)} borrowed`, body: <Chain id={s.chain_id} name={s.name || short(s.address)} /> })
    }
    return hits.sort((a, b) => b.size - a.size)
  }, [assets, hubs, spokes, lines])

  const term = q.trim().toLowerCase()
  const isAddress = ADDRESS.test(term)
  const looksLikeAddress = term.startsWith('0x') && !isAddress
  const shown = useMemo(() => {
    if (isAddress) return [{ key: 'w', href: `/wallet/${term}`, group: 'Wallets', text: term, size: 0, body: <span className="mono">{short(term)}</span>, note: 'Open this wallet' } as Hit, ...all.filter((h) => h.href.endsWith(term))]
    const match = term ? all.filter((h) => h.text.includes(term)) : all
    return ['Tokens', 'Pools', 'Markets'].flatMap((g) => match.filter((h) => h.group === g).slice(0, term ? 6 : 3))
  }, [all, term, isAddress])
  const go = (href: string) => { navigate(href); setOpen(false); setQ(''); input.current?.blur() }
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') { setOpen(false); input.current?.blur() }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(0, Math.min(shown.length - 1, s + (e.key === 'ArrowDown' ? 1 : -1)))) }
    if (e.key === 'Enter' && shown[sel]) go(shown[sel].href)
  }

  return (
    <div className="search" onBlur={(e) => { if (!e.currentTarget.contains(e.relatedTarget)) setOpen(false) }}>
      <label className="search__box">
        <Icon name="search" />
        <input ref={input} type="search" role="combobox" aria-expanded={open} aria-controls="search-results" aria-autocomplete="list" autoComplete="off" spellCheck={false}
          placeholder="Search a token, pool, market or address" aria-label="Search a token, pool, market or address"
          value={q} onChange={(e) => { setQ(e.target.value); setSel(0); setOpen(true) }} onFocus={() => setOpen(true)} onKeyDown={onKey} />
        <kbd aria-hidden="true">/</kbd>
      </label>
      {open && (
        <div className="results" id="search-results" role="listbox" aria-label="Results">
          {!assets ? <p className="muted">Loading…</p>
            : looksLikeAddress ? <p>That does not look like an address. An address starts with 0x and has 42 characters.</p>
            : shown.length === 0 ? <p>Nothing on Aave v4 is called "{q.trim()}". Try a token symbol, a pool or market name, or paste an address.</p>
            : shown.map((h, i) => (
              <div key={h.key}>
                {(i === 0 || shown[i - 1].group !== h.group) && <h2>{h.group}</h2>}
                <a href={h.href} role="option" aria-selected={i === sel} onMouseEnter={() => setSel(i)} onClick={(e) => { e.preventDefault(); go(h.href) }}>
                  <span className="pills">{h.body}</span><span className="caption">{h.note}</span>
                </a>
              </div>
            ))}
          {!term && assets && <><h2>Wallets</h2><p className="muted">Paste an address to open its page</p></>}
        </div>
      )}
    </div>
  )
}

function ThemeSwitch() {
  const [theme, setTheme] = useState(() => document.documentElement.dataset.theme ?? '')
  const pick = (t: string) => {
    if (t) document.documentElement.dataset.theme = t; else delete document.documentElement.dataset.theme
    try { localStorage.setItem('aw-theme', t) } catch { /* private mode: the choice lasts for this page */ }
    setTheme(t)
  }
  return (
    <span className="seg" role="group" aria-label="Theme">
      <button type="button" aria-pressed={theme === ''} onClick={() => pick('')}>System</button>
      <button type="button" aria-pressed={theme === 'light'} onClick={() => pick('light')}><Icon name="sun" />Day</button>
      <button type="button" aria-pressed={theme === 'dark'} onClick={() => pick('dark')}><Icon name="moon" />Night</button>
    </span>
  )
}

export default function App() {
  const { path } = useLocation()
  useLive()
  const m = (re: RegExp) => path.match(re)
  const token = m(/^\/token\/([^/]+)$/), pool = m(/^\/pool\/(\d+)\/(0x[0-9a-fA-F]{40})$/), market = m(/^\/market\/(\d+)\/(0x[0-9a-fA-F]{40})$/), wallet = m(/^\/wallet\/(0x[0-9a-fA-F]{40})$/)
  const section = path === '/' ? '/' : token || pool || path === '/rates' ? '/rates' : market || path === '/close-calls' ? '/close-calls' : ''
  useEffect(() => { scrollTo(0, 0) }, [path])

  const page =
    path === '/' ? <Now /> :
    path === '/rates' ? <Rates /> :
    path === '/close-calls' ? <CloseCalls /> :
    token ? <TokenPage key={path} symbol={decodeURIComponent(token[1]).toLowerCase()} /> :
    pool ? <Pool key={path} chain={Number(pool[1])} hub={pool[2].toLowerCase()} /> :
    market ? <Market key={path} chain={Number(market[1])} spoke={market[2].toLowerCase()} /> :
    wallet ? <WalletPage key={path} address={wallet[1].toLowerCase()} /> :
    <><h1>There is no page here</h1><p className="muted">{path} is not a page of aave.watch. Try the search, or <a href="/">go to Now</a>.</p></>

  return (
    <>
      <a className="skip" href="#main">Skip to content</a>
      <header className="top">
        <div className="top__in">
          <a className="logo" href="/" aria-label="aave.watch, the Now page"><Mark />aave.watch</a>
          <nav className="menu" aria-label="Pages">
            {MENU.map(([to, label]) => <a key={to} href={to} aria-current={section === to ? 'page' : undefined}>{label}</a>)}
          </nav>
          <Search />
        </div>
      </header>
      <main id="main">{page}</main>
      <footer className="foot">
        <span>aave.watch is an independent project. Not affiliated with Aave Labs or the Aave DAO.</span>
        <ThemeSwitch />
      </footer>
    </>
  )
}
