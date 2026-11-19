// The pure part of the share images and link-preview tags. Runs without Cloudflare credentials:
// `pnpm --filter worker test:share`. Not named *.test.ts: vitest (index.spec.ts) runs in workerd and needs the remote API binding.
import assert from "node:assert/strict";
import { test } from "node:test";
import { closeLine, homeLine, ogRoute, page, pct, route, short, svg, tokenLine, totalsLine, usd, walletLine } from "../src/share.ts";

const HUB = "0xcca852bc40e560adc3b1cc58ca5b55638ce826c9", WALLET = "0x1b36972588d214aea7b8f5322f349246bf571cd6";
const api: Record<string, any> = {
	"/icons/tokens.json": { "1:0xdac1": { symbol: "USDT", file: "tokens/usdt.svg" } },
	"/v1/hubs?chain=1": [{ address: HUB, name: "Core" }],
	"/v1/spokes?chain=1": [{ address: HUB, name: "" }],
	[`/v1/hub-assets?chain=1&hub=${HUB}`]: [{ added_usd: 100e6, owed_usd: 37.1e6 }, { added_usd: 37.1e6, owed_usd: 0 }],
	"/v1/hub-assets": [
		{ symbol: "USDT", added_usd: 1, supply_apy_7d: 0.03, drawn_rate_apr: 0.0449 },
		{ symbol: "USDt", added_usd: 1, supply_apy_7d: 0.0481, drawn_rate_apr: 0.0481 },
	],
};
const get = async (path: string) => api[path] ?? null;
const down = async () => null;

test("number formats", () => {
	assert.deepEqual([1.31e9, 137.1e6, 788e3, 2.54, 999_999_999].map(usd), ["$1.31B", "$137.1M", "$788K", "$2.54", "$1.00B"]);
	assert.deepEqual([pct(0.0496), pct(0.000002), pct(0), pct(0.214, 1)], ["4.96%", "under 0.01%", "0.00%", "21.4%"]);
	assert.equal(short(WALLET), "0x1b36…1cd6");
});

test("paths", () => {
	assert.deepEqual(route("/"), { kind: "home" });
	assert.deepEqual(route("/token/USDt/"), { kind: "token", symbol: "usdt" });
	assert.deepEqual(route(`/pool/1/${HUB.toUpperCase().replace("0X", "0x")}`), { kind: "pool", chain: "1", address: HUB });
	assert.deepEqual(ogRoute(`/og/wallet/${WALLET}.png`), { kind: "wallet", address: WALLET });
	assert.deepEqual(ogRoute("/og/home.png"), { kind: "home" });
	for (const bad of ["/pool/2/" + HUB, "/wallet/0x12", "/token/a<b", "/rates/x", "/nothing", "/token/usdc/more"]) assert.equal(route(bad), null, bad);
	for (const bad of ["/og/home", "/og/.png", "/og/nothing.png"]) assert.equal(ogRoute(bad), null, bad);
});

test("live lines", () => {
	const summary = [{ supplied_usd: 1e9, borrowed_usd: 400e6, positions_below_1_1: "228", liquidatable: "1" }, { supplied_usd: 310e6, borrowed_usd: 118e6, positions_below_1_1: "77", liquidatable: "0" }];
	assert.equal(homeLine(summary), "Aave v4 holds $1.31B, and $518.0M of it is out on loan.");
	assert.equal(closeLine(summary), "305 loans are close to liquidation, and 1 can be liquidated now.");
	assert.equal(closeLine([{ positions_below_1_1: "0", liquidatable: "0" }]), "All quiet. No loan is close to liquidation.");
	assert.equal(tokenLine(api["/v1/hub-assets"]), "Earns up to 4.81% (7 days). Borrow from 4.49%.");
	assert.equal(tokenLine([{ added_usd: 6.01e6, supply_apy_7d: 0, drawn_rate_apr: 0 }]), "$6.0M deposited across Aave v4.");
	assert.equal(totalsLine(137.1e6, 788e3), "$137.1M deposited, $788K borrowed.");
});

test("wallet line: the lowest health factor of every market decides", () => {
	const at = (health_factor: number | null, debt_usd = 1) => ({ health_factor, collateral_usd: 5, debt_usd });
	assert.equal(walletLine([at(2), at(1 / (1 - 0.214))]), "21.4% from liquidation");
	assert.equal(walletLine([at(2), at(0.98)]), "Can be liquidated now");
	assert.equal(walletLine([at(null, 0)]), "No loan");
	assert.equal(walletLine([]), "Nothing in Aave v4");
	assert.equal(walletLine([{ health_factor: null, collateral_usd: 0, debt_usd: 0 }]), "Nothing in Aave v4");
});

test("pages: names, figures, and what happens without the API", async () => {
	const token = await page(route("/token/usdt"), get, true);
	assert.deepEqual([token.path, token.og, token.title, token.image.logo, token.live], ["/token/usdt", "/og/token/usdt.png", "USDT on Aave · aave.watch", "tokens/usdt.svg", true]);
	assert.equal((await page(route("/token/nope"), get, true)).found, false);

	const pool = await page(route(`/pool/1/${HUB}`), get, true);
	assert.deepEqual([pool.title, pool.image.label, pool.image.title, pool.image.line], ["Core pool on Ethereum · aave.watch", "Pool on Ethereum", "Core", "$137.1M deposited, $37.1M borrowed."]);
	assert.equal((await page(route(`/pool/1/${WALLET}`), get, true)).found, false);
	const offline = await page(route(`/pool/1/${HUB}`), down, true);
	assert.deepEqual([offline.found, offline.live, offline.image.title, offline.image.line], [true, false, "0xcca8…26c9", "What is inside, how much is lent out, and who borrows it."]);

	const market = await page(route(`/market/1/${HUB}`), get, false);
	assert.equal(market.title, "Market 0xcca8…26c9 on Ethereum · aave.watch");

	// a wallet whose reads fail gets the fixed line, not "Nothing in Aave v4"
	assert.equal((await page(route(`/wallet/${WALLET}`), down, true)).live, false);
	assert.equal((await page(route(`/wallet/${WALLET}`), async () => [], true)).image.line, "Nothing in Aave v4");

	const other = await page(route("/about"), down, false, "/about");
	assert.deepEqual([other.path, other.og, other.title], ["/about", "/og/home.png", "aave.watch: Aave, live"]);
});

test("the template never says Hub, Spoke or HF, and escapes its text", async () => {
	for (const path of ["/", "/rates", "/close-calls", "/token/usdt", `/pool/1/${HUB}`, `/market/1/${HUB}`, `/wallet/${WALLET}`]) {
		const p = await page(route(path), get, true);
		assert.doesNotMatch(svg(p.image) + p.title + p.description, /\b(hub|spoke|hf|health factor)\b/i, path);
	}
	assert.match(svg({ title: "a<b & c", line: "" }), /a&lt;b &amp; c/);
});
