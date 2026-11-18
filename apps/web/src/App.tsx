import { type FormEvent, useState } from 'react'
import { useApi } from './api.ts'
import { Account } from './pages/Account.tsx'
import { Adoption } from './pages/Adoption.tsx'
import { Feed } from './pages/Feed.tsx'
import { Hubs } from './pages/Hubs.tsx'
import { Risk } from './pages/Risk.tsx'
import { Spokes } from './pages/Spokes.tsx'
import { navigate, useChain, useLocation, withChain } from './router.ts'
import { CHAIN_NAMES } from './lib.ts'
import { Icon } from './ui.tsx'

const NAV = [['/', 'Risk'], ['/hubs', 'Hubs'], ['/spokes', 'Spokes'], ['/adoption', 'Adoption'], ['/feed', 'Live feed']] as const
const FALLBACK = Object.entries(CHAIN_NAMES).map(([chain_id, name]) => ({ chain_id: Number(chain_id), name }))

function ThemeSwitch() {
  const [theme, setTheme] = useState(() => document.documentElement.dataset.theme ?? 'dark')
  const pick = (t: string) => {
    document.documentElement.dataset.theme = t
    try { localStorage.setItem('aw-theme', t) } catch { /* private mode: the choice lasts for this page */ }
    setTheme(t)
  }
  return (
    <fieldset className="seg">
      <legend className="visually-hidden">Theme</legend>
      {['dark', 'light'].map((t) => (
        <span key={t} style={{ display: 'contents' }}>
          <input type="radio" name="theme" id={`th-${t}`} checked={theme === t} onChange={() => pick(t)} />
          <label htmlFor={`th-${t}`}>{t === 'dark' ? 'Dark' : 'Light'}</label>
        </span>
      ))}
    </fieldset>
  )
}

function ChainPicker({ chain, path }: { chain: number; path: string }) {
  const chains = useApi<{ data: { chain_id: number; name: string }[] }>('/v1/chains').data?.data
  const list = chains?.length ? chains : FALLBACK
  return (
    <fieldset className="seg">
      <legend className="visually-hidden">Chain</legend>
      {[{ chain_id: 0, name: 'All' }, ...list].map((c) => (
        <span key={c.chain_id} style={{ display: 'contents' }}>
          <input type="radio" name="chain" id={`ch-${c.chain_id}`} checked={chain === Number(c.chain_id)} onChange={() => navigate(withChain(path, Number(c.chain_id)))} />
          <label htmlFor={`ch-${c.chain_id}`}>{c.name}</label>
        </span>
      ))}
    </fieldset>
  )
}

function AccountSearch({ chain }: { chain: number }) {
  const [value, setValue] = useState('')
  const [error, setError] = useState('')
  const submit = (e: FormEvent) => {
    e.preventDefault()
    const a = value.trim().toLowerCase()
    if (!/^0x[0-9a-f]{40}$/.test(a)) return setError('Enter a full address: 0x and 40 hex characters.')
    setError('')
    navigate(withChain(`/account/${a}`, chain))
  }
  return (
    <form className="search" role="search" onSubmit={submit}>
      <Icon name="search" />
      <label className="visually-hidden" htmlFor="q">Account address</label>
      <input className="input mono" id="q" type="search" placeholder="0x… account" autoComplete="off" spellCheck={false} value={value}
        onChange={(e) => setValue(e.target.value)} aria-invalid={error ? true : undefined} aria-describedby={error ? 'q-help' : undefined} />
      {error && <p className="help help--error" id="q-help" role="alert"><Icon name="alert" />{error}</p>}
    </form>
  )
}

export default function App() {
  const { path } = useLocation()
  const chain = useChain()
  const account = path.match(/^\/account\/(0x[0-9a-fA-F]{40})$/)?.[1]?.toLowerCase()
  const page =
    path === '/' ? <Risk chain={chain} /> :
    path === '/hubs' ? <Hubs chain={chain} /> :
    path === '/spokes' ? <Spokes chain={chain} /> :
    path === '/adoption' ? <Adoption chain={chain} /> :
    path === '/feed' ? <Feed chain={chain} /> :
    account ? <Account chain={chain} user={account} /> :
    (
      <div className="page">
        <header><h1 className="t-h1">No page at this address</h1><p className="muted">{path} is not a page of aave.watch.</p></header>
        <p><a href={withChain('/', chain)}>Go to Risk</a></p>
      </div>
    )

  return (
    <>
      <a className="skip" href="#main">Skip to content</a>
      <header className="top">
        <div className="top__in">
          <a className="wordmark" href={withChain('/', chain)} aria-label="aave.watch, Risk page">aave<b>.</b>watch</a>
          <nav aria-label="Pages">
            {NAV.map(([to, label]) => <a key={to} href={withChain(to, chain)} aria-current={path === to ? 'page' : undefined}>{label}</a>)}
          </nav>
          <div className="top__tools">
            <AccountSearch chain={chain} />
            <ChainPicker chain={chain} path={path} />
            <ThemeSwitch />
          </div>
        </div>
      </header>
      <main id="main">
        {page}
        <footer className="foot page" style={{ paddingBlock: 'var(--s-5)' }}>
          <p>aave.watch is an independent project. Not affiliated with Aave Labs or the Aave DAO.</p>
        </footer>
      </main>
    </>
  )
}
