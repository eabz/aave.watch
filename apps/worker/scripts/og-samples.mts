// Renders share images in Node with the Worker's template, renderer and fonts, to look at them
// without deploying: `node scripts/og-samples.mts <out dir> [api, default http://127.0.0.1:8080] <paths...>`
// where a path is what the Worker serves, such as /og/token/usdc.png.
import { initWasm, Resvg } from "@resvg/resvg-wasm";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dataUri, ogRoute, page, svg } from "../src/share.ts";

const [out, ...rest] = process.argv.slice(2);
const api = rest[0]?.startsWith("http") ? rest.shift()! : "http://127.0.0.1:8080";
const here = new URL(".", import.meta.url).pathname, icons = here + "../../web/public/icons/";
const get = async (path: string) => {
	try {
		if (!path.startsWith("/v1/")) return JSON.parse(readFileSync(icons + path.replace("/icons/", ""), "utf8"));
		const answer = await fetch(api + path, { signal: AbortSignal.timeout(3000) });
		return answer.ok ? (await answer.json()).data : null;
	} catch { return null; }
};
const logo = (file?: string) => file && dataUri(file, readFileSync(icons + file));

await initWasm(readFileSync(here + "../node_modules/@resvg/resvg-wasm/index_bg.wasm"));
const fontBuffers = ["onest-700", "figtree-400", "figtree-600"].map((f) => readFileSync(`${here}../src/fonts/${f}.ttf`));
mkdirSync(out, { recursive: true });
for (const path of rest) {
	const route = ogRoute(path), p = route && (await page(route, get, true));
	if (!p?.found) { console.log(`${path}: 404`); continue; }
	const started = performance.now();
	const png = new Resvg(svg(p.image, logo(p.image.logo), logo(p.image.labelLogo)), { font: { fontBuffers, loadSystemFonts: false, defaultFontFamily: "Figtree" } }).render().asPng();
	const file = `${out}/${path.slice(4).replaceAll("/", "-")}`;
	writeFileSync(file, png);
	console.log(`${path}: ${p.live ? "live" : "fixed"} line "${p.image.line}", ${png.length} bytes, ${Math.round(performance.now() - started)} ms -> ${file}`);
}
