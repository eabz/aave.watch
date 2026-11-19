// Link previews: what each page is called, the sentence under it, and its share image
// (docs/branding.md, "Share images and favicon"). Pure: no Worker APIs and no imports, so
// `node --test` and scripts/og-samples.mts run this same file.

export const SITE = "https://aave.watch";
// The chains of registry/chains.toml. A constant so a page keeps its chain's name when the API is down.
const CHAINS: Record<string, string> = { 1: "Ethereum", 8453: "Base", 43114: "Avalanche", 5042: "Arc" };

export type Route =
	| { kind: "home" | "rates" | "close-calls" }
	| { kind: "token"; symbol: string }
	| { kind: "pool" | "market"; chain: string; address: string }
	| { kind: "wallet"; address: string };

/** What the template draws. `logo` and `labelLogo` are files under /icons/. */
export type Image = { label?: string; labelLogo?: string; title: string; line: string; logo?: string };
export type Page = {
	/** Canonical path: lowercase, no trailing slash. */
	path: string;
	/** Path of the share image. */
	og: string;
	title: string;
	description: string;
	image: Image;
	/** False when the path names a token, pool or market that does not exist. */
	found: boolean;
	/** True when `image.line` was written from live figures. */
	live: boolean;
};
/** `/v1/...` answers the endpoint's `data` array, anything else the JSON file; null when it failed. */
export type Get = (path: string) => Promise<any>;

// --- paths ---

/** The page a path belongs to, or null. Case and a trailing slash do not matter. */
export function route(pathname: string): Route | null {
	const [, kind = "", a = "", b = "", extra] = pathname.toLowerCase().replace(/\/+$/, "").split("/");
	const address = (s: string) => /^0x[0-9a-f]{40}$/.test(s);
	if (extra !== undefined) return null;
	if (!b) {
		if (!a && (kind === "" || kind === "rates" || kind === "close-calls")) return { kind: kind || "home" };
		if (kind === "token" && /^[a-z0-9.-]{1,40}$/.test(a)) return { kind, symbol: a };
		if (kind === "wallet" && address(a)) return { kind, address: a };
	} else if ((kind === "pool" || kind === "market") && CHAINS[a] && address(b)) return { kind, chain: a, address: b };
	return null;
}

/** `/og/token/usdc.png` is the image of `/token/usdc`; `/og/home.png` the image of `/`. */
export function ogRoute(pathname: string): Route | null {
	const m = /^\/og\/(.+)\.png$/.exec(pathname.toLowerCase());
	return m && m[1] !== "" ? route(m[1] === "home" ? "/" : "/" + m[1]) : null;
}

// --- number formats (docs/branding.md) ---

export function usd(v: number): string {
	if (v >= 999.995e6) return `$${(v / 1e9).toFixed(2)}B`;
	if (v >= 999.95e3) return `$${(v / 1e6).toFixed(1)}M`;
	if (v >= 999.995) return `$${(v / 1e3).toFixed(0)}K`;
	return `$${v.toFixed(2)}`;
}
export function pct(fraction: number, digits = 2): string {
	const floor = 10 ** -digits;
	return fraction > 0 && fraction * 100 < floor ? `under ${floor}%` : `${(fraction * 100).toFixed(digits)}%`;
}
export const short = (address: string) => `${address.slice(0, 6)}…${address.slice(-4)}`;

// --- the live line of each image, from API rows ---

const sum = (rows: any[], field: string) => rows.reduce((t, r) => t + Number(r[field] ?? 0), 0);

/** `/v1/summary` rows. */
export const homeLine = (summary: any[]) =>
	`Aave v4 holds ${usd(sum(summary, "supplied_usd"))}, and ${usd(sum(summary, "borrowed_usd"))} of it is out on loan.`;

/** `/v1/summary` rows. A loan is one wallet in one market; "close" is the API's health factor below 1.1. */
export function closeLine(summary: any[]): string {
	const close = sum(summary, "positions_below_1_1"), now = sum(summary, "liquidatable");
	const n = (count: number) => (count === 1 ? "1 loan is" : `${count.toLocaleString("en-US")} loans are`);
	if (!close) return "All quiet. No loan is close to liquidation.";
	return `${n(close)} close to liquidation${now ? `, and ${now.toLocaleString("en-US")} can be liquidated now` : ""}.`;
}

/** `/v1/hub-assets` rows: the biggest asset that is lent out. */
export function ratesLine(assets: any[]): string | null {
	const a = assets.filter((r) => r.drawn_rate_apr > 0 && r.supply_apy_7d != null).sort((x, y) => y.added_usd - x.added_usd)[0];
	if (!a) return null;
	return `${a.symbol} earns ${pct(a.supply_apy_7d)} and costs ${pct(a.drawn_rate_apr)} to borrow on ${CHAINS[a["s.chain_id"]]}.`;
}

/** `/v1/hub-assets` rows of one token: best earn rate (7 days) and cheapest borrow rate across pools. */
export function tokenLine(rows: any[]): string | null {
	const earn = Math.max(0, ...rows.map((r) => r.supply_apy_7d ?? 0));
	// A rate of 0 is an asset nobody borrows (collateral only, or just listed), not a free loan.
	const borrow = Math.min(...rows.map((r) => r.drawn_rate_apr).filter((r) => r > 0));
	const deposited = sum(rows, "added_usd");
	if (!earn || borrow === Infinity) return deposited ? `${usd(deposited)} deposited across Aave v4.` : null;
	return `Earns ${earn < 0.0001 ? "" : "up to "}${pct(earn)} (7 days). Borrow from ${pct(borrow)}.`;
}

export const totalsLine = (deposited: number, borrowed: number) => `${usd(deposited)} deposited, ${usd(borrowed)} borrowed.`;

/** `/v1/account` rows of one wallet, every chain together. */
export function walletLine(accounts: any[]): string {
	if (!accounts.some((a) => a.collateral_usd > 0 || a.debt_usd > 0)) return "Nothing in Aave v4";
	const factors = accounts.map((a) => a.health_factor).filter((h) => h != null);
	if (!factors.length) return "No loan";
	const lowest = Math.min(...factors);
	return lowest < 1 ? "Can be liquidated now" : `${pct(1 - 1 / lowest, 1)} from liquidation`;
}

// --- one page ---

const HOME = {
	title: "aave.watch: Aave, live",
	description: "What is in Aave, what each asset earns and costs, and who is close to liquidation, live on every chain Aave v4 runs on.",
	image: { title: "Aave, live.", line: "What is in Aave, what it earns, and who is close to liquidation." },
};

/**
 * Title, description and share image of a route (null: any other path, which gets the home text).
 * `figures` false is the HTML case: at most one lookup, for the name. True is the image case: it
 * also reads the live line. Every lookup may fail; the text then falls back to the fixed wording.
 */
export async function page(r: Route | null, get: Get, figures: boolean, pathname = "/"): Promise<Page> {
	const named = (title: string) => `${title} · aave.watch`;
	const done = (path: string, title: string, description: string, image: Image, line?: string | null | false | 0, found = true): Page =>
		({ path, og: `/og${path === "/" ? "/home" : path}.png`, title, description, found, live: !!line, image: line ? { ...image, line } : image });
	const live = async (path: string) => (figures ? await get(path) : null);

	switch (r?.kind) {
		case "rates": {
			const rows = await live("/v1/hub-assets");
			const line = "What every asset earns and what it costs to borrow on Aave v4.";
			return done("/rates", named("Rates on Aave"), line, { title: "Rates", line }, rows && ratesLine(rows));
		}
		case "close-calls": {
			const rows = await live("/v1/summary");
			const line = "Wallets close to liquidation on Aave v4, biggest loans first.";
			return done("/close-calls", named("Close calls on Aave"), line, { title: "Close calls", line }, rows?.length && closeLine(rows));
		}
		case "token": {
			const [tokens, assets] = await Promise.all([get("/icons/tokens.json"), live("/v1/hub-assets")]);
			const listed: any = Object.values(tokens ?? {}).find((t: any) => t.symbol.toLowerCase() === r.symbol);
			const rows = (assets ?? []).filter((a: any) => a.symbol.toLowerCase() === r.symbol);
			const symbol = listed?.symbol ?? rows[0]?.symbol ?? r.symbol.toUpperCase();
			return done(`/token/${r.symbol}`, named(`${symbol} on Aave`), `What ${symbol} earns and what it costs to borrow across Aave v4.`,
				{ label: "Token", title: symbol, line: "What it earns and what it costs to borrow across Aave v4.", logo: listed?.file },
				rows.length && tokenLine(rows), !!listed || rows.length > 0);
		}
		case "pool": {
			const [hubs, assets] = await Promise.all([get(`/v1/hubs?chain=${r.chain}`), live(`/v1/hub-assets?chain=${r.chain}&hub=${r.address}`)]);
			const name = hubs?.find((h: any) => h.address === r.address)?.name, chain = CHAINS[r.chain];
			const title = name ? `${name} pool` : `Pool ${short(r.address)}`;
			return done(`/pool/${r.chain}/${r.address}`, named(`${title} on ${chain}`),
				`What is inside the ${name ? `${name} pool` : "pool"} on ${chain}, how much is lent out, and who borrows it.`,
				{ label: `Pool on ${chain}`, labelLogo: `chains/${r.chain}.svg`, title: name ?? short(r.address), line: "What is inside, how much is lent out, and who borrows it." },
				assets?.length && totalsLine(sum(assets, "added_usd"), sum(assets, "owed_usd")), !hubs || !!name);
		}
		case "market": {
			const [spokes, reserves] = await Promise.all([get(`/v1/spokes?chain=${r.chain}`), live(`/v1/reserves?chain=${r.chain}&spoke=${r.address}`)]);
			const spoke = spokes?.find((s: any) => s.address === r.address), chain = CHAINS[r.chain];
			const name = spoke?.name || short(r.address);
			return done(`/market/${r.chain}/${r.address}`, named(`${spoke?.name ? `${name} market` : `Market ${name}`} on ${chain}`),
				`What can be deposited and borrowed in ${spoke?.name ? `the ${name} market` : `market ${name}`} on ${chain}, and how safe the loans are.`,
				{ label: `Market on ${chain}`, labelLogo: `chains/${r.chain}.svg`, title: name, line: "What can be deposited and borrowed, and how safe the loans are." },
				reserves?.length && totalsLine(sum(reserves, "supplied_usd"), sum(reserves, "debt_usd")), !spokes || !!spoke);
		}
		case "wallet": {
			const name = short(r.address);
			// The path has no chain: the wallet is read on every chain. One failed read and the line is left out.
			const chains = figures ? await Promise.all(Object.keys(CHAINS).map((c) => get(`/v1/account?chain=${c}&user=${r.address}`))) : [null];
			return done(`/wallet/${r.address}`, named(`Wallet ${name} on Aave`),
				`How far wallet ${name} is from liquidation, what it holds and what it owes on Aave v4.`,
				{ label: "Wallet", title: name, line: "How far it is from liquidation, what it holds and what it owes." },
				chains.every(Boolean) && walletLine(chains.flat()));
		}
		default: {
			const rows = await live("/v1/summary");
			const page = done("/", HOME.title, HOME.description, HOME.image, rows?.length && homeLine(rows));
			// An unknown path keeps its own address, so a page added to the app later is not declared a copy of home.
			return r ? page : { ...page, path: pathname };
		}
	}
}

// --- the template: the look of every share image, defined once ---

export const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

/** A logo file's bytes as a data URI the template can embed. SVG or PNG; the renderer cannot read WebP. */
export function dataUri(file: string, bytes: Uint8Array): string {
	let binary = "";
	for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	return `data:${file.endsWith(".svg") ? "image/svg+xml" : "image/png"};base64,${btoa(binary)}`;
}

const mark = (x: number, y: number, size: number) =>
	`<g transform="translate(${x} ${y}) scale(${size / 48})"><rect width="48" height="48" rx="14" fill="#ff5fa2"/><polyline points="11,17 18,32 24,21 30,32 37,15" fill="none" stroke="#121016" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"/></g>`;

/** 1200 by 630, night theme. `logo` and `labelLogo` are data URIs (see `dataUri`). Fonts: Onest 700, Figtree 400 and 600. */
export function svg({ label, title, line }: Image, logo?: string, labelLogo?: string): string {
	// a logo takes 160 of the width, so a long symbol (PT-sUSDE-7MAY2026) drops a size sooner
	const size = title.length > (logo ? 16 : 22) ? 64 : title.length > 12 ? 92 : 124, tx = logo ? 240 : 80;
	const lw = label ? label.length * 14.5 + (labelLogo ? 84 : 48) : 0;
	return `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#121016"/>
  <polyline points="700,250 830,470 940,310 1050,470 1180,190" fill="none" stroke="#ff5fa2" stroke-opacity="0.16" stroke-width="56" stroke-linecap="round" stroke-linejoin="round"/>
  ${mark(80, 64, 60)}<text x="154" y="109" font-family="Onest" font-weight="700" font-size="42" letter-spacing="-1.9" fill="#f4f2f8">aave.watch</text>
  ${label ? `<rect x="80" y="214" width="${lw}" height="52" rx="26" fill="#26232e"/>${labelLogo ? `<image href="${labelLogo}" x="96" y="224" width="32" height="32"/>` : ""}<text x="${labelLogo ? 140 : 104}" y="249" font-family="Figtree" font-weight="600" font-size="26" fill="#f4f2f8">${esc(label)}</text>` : ""}
  ${logo ? `<circle cx="150" cy="364" r="71" fill="#322e3d"/><clipPath id="c"><circle cx="150" cy="364" r="68"/></clipPath><image href="${logo}" x="82" y="296" width="136" height="136" clip-path="url(#c)"/>` : ""}
  <text x="${tx}" y="${364 + size * 0.34}" font-family="Onest" font-weight="700" font-size="${size}" letter-spacing="${-size * 0.025}" fill="#f4f2f8">${esc(title)}</text>
  <text x="80" y="492" font-family="Figtree" font-weight="400" font-size="32" fill="#aaa5b8">${esc(line)}</text>
  <rect x="80" y="538" width="1040" height="1" fill="#322e3d"/>
  <text x="80" y="580" font-family="Figtree" font-weight="400" font-size="22" fill="#aaa5b8">Independent. Not affiliated with Aave Labs or the Aave DAO.</text>
</svg>`;
}
