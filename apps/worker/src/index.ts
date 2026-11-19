// Three jobs (docs/architecture.md, "API Worker"):
//   /v1/*       API proxy and SSE relay. The API is the Rust daemon behind the tunnel; see docs/api.md.
//   /og/*.png   share images, rendered on request from the template in share.ts.
//   pages       the web build's index.html, with the head rewritten for the path.
// Static files (/assets, /icons, /fonts, /favicon.svg) never reach this code; see wrangler.jsonc.
import { initWasm, Resvg } from "@resvg/resvg-wasm";
import wasm from "@resvg/resvg-wasm/index_bg.wasm";
import figtree400 from "./fonts/figtree-400.ttf";
import figtree600 from "./fonts/figtree-600.ttf";
import onest700 from "./fonts/onest-700.ttf";
import { dataUri, esc, type Get, ogRoute, page, type Page, route, SITE, svg } from "./share";

type Vars = {
	/**
	 * Workers VPC service "aave-watch-api": the API daemon on the box, reached through the Cloudflare
	 * Tunnel "aave-watch-box". There is no public hostname for it. Absent in tests.
	 */
	API?: Fetcher;
	/** The web build (apps/web/dist). */
	ASSETS: Fetcher;
};

// Where the API listens on the box, as seen from the tunnel's end.
const API = "http://127.0.0.1:8080";

const fail = (status: number, error: string) =>
	Response.json({ error }, { status, headers: { "cache-control": "no-store" } });

export default {
	async fetch(request, env, ctx): Promise<Response> {
		const url = new URL(request.url);
		if (url.pathname.startsWith("/v1/")) return proxy(request, env, ctx, url);
		if (request.method !== "GET" && request.method !== "HEAD") return env.ASSETS.fetch(request);
		if (url.pathname.startsWith("/og/")) return shareImage(env, ctx, url);

		const response = await env.ASSETS.fetch(request);
		if (!response.headers.get("content-type")?.includes("text/html")) return response;
		return head(response, await page(route(url.pathname), lookup(env, ctx, url.origin), false, url.pathname));
	},
} satisfies ExportedHandler<Vars>;

async function proxy(request: Request, env: Vars, ctx: ExecutionContext, url: URL): Promise<Response> {
	if (request.method !== "GET" && request.method !== "HEAD") {
		return fail(405, "The API is read-only. Use GET.");
	}
	if (!env.API) {
		return fail(503, "The API is not connected yet.");
	}
	const isStream = url.pathname === "/v1/stream";

	// Answers carry Cache-Control from the API; the edge cache honours it.
	const cache = caches.default;
	if (!isStream) {
		const hit = await cache.match(request);
		if (hit) return hit;
	}

	const headers = new Headers({ accept: request.headers.get("accept") ?? "*/*" });
	const lastEventId = request.headers.get("last-event-id");
	if (lastEventId) headers.set("last-event-id", lastEventId);

	let upstream: Response;
	try {
		// Only the path and query go upstream: no cookies, no client headers beyond the two above.
		upstream = await env.API.fetch(API + url.pathname + url.search, { headers, signal: request.signal });
	} catch {
		return fail(502, "The API did not answer. Figures on screen are from the last successful load.");
	}

	// The body is passed through as it arrives, which is what keeps /v1/stream live.
	const response = new Response(upstream.body, upstream);
	if (!isStream && upstream.ok && request.method === "GET") {
		ctx.waitUntil(cache.put(request, response.clone()));
	}
	return response;
}

/**
 * Reads for share.ts: an API endpoint's `data`, or a JSON file of the web build. Null on any failure,
 * so a slow or broken API costs a preview its live line and nothing else. API answers go through the
 * same edge cache entries as the proxy, for as long as the API's Cache-Control says.
 */
const lookup = (env: Vars, ctx: ExecutionContext, origin: string): Get => async (path) => {
	try {
		if (!path.startsWith("/v1/")) {
			const file = await env.ASSETS.fetch(origin + path);
			// A missing file answers index.html (single-page-application fallback), hence the type check.
			return file.headers.get("content-type")?.includes("json") ? await file.json() : null;
		}
		if (!env.API) return null;
		const key = new Request(origin + path);
		let answer = await caches.default.match(key);
		if (!answer) {
			answer = await env.API.fetch(API + path, { signal: AbortSignal.timeout(3000) });
			if (answer.ok) ctx.waitUntil(caches.default.put(key, answer.clone()));
		}
		return answer.ok ? ((await answer.json()) as { data: unknown }).data : null;
	} catch {
		return null;
	}
};

// The renderer is started on the first image, not at startup: a Worker has one second to start.
let renderer: Promise<void> | undefined;
const fonts = [onest700, figtree400, figtree600].map((font) => new Uint8Array(font));

async function shareImage(env: Vars, ctx: ExecutionContext, url: URL): Promise<Response> {
	const text = (status: number, body: string) =>
		new Response(body, { status, headers: { "cache-control": "public, max-age=60" } });
	const r = ogRoute(url.pathname);
	if (!r) return text(404, "No such share image.");

	// Keyed without the query string, so one path is rendered at most once an hour.
	const key = new Request(url.origin + url.pathname.toLowerCase());
	const hit = await caches.default.match(key);
	if (hit) return hit;

	const p = await page(r, lookup(env, ctx, url.origin), true);
	if (!p.found) return text(404, "No such token, pool or market.");

	const logo = async (file?: string) => {
		const icon = file && (await env.ASSETS.fetch(`${url.origin}/icons/${file}`).catch(() => null));
		return icon && icon.headers.get("content-type")?.startsWith("image/") ? dataUri(file, new Uint8Array(await icon.arrayBuffer())) : undefined;
	};
	const [tokenLogo, chainLogo] = await Promise.all([logo(p.image.logo), logo(p.image.labelLogo)]);

	await (renderer ??= initWasm(wasm));
	const drawing = new Resvg(svg(p.image, tokenLogo, chainLogo), { font: { fontBuffers: fonts, loadSystemFonts: false, defaultFontFamily: "Figtree" } });
	const rendered = drawing.render();
	const png = rendered.asPng();
	rendered.free();
	drawing.free();

	// Without its live line (the API did not answer) an image is kept for a minute, not an hour.
	const response = new Response(png, {
		headers: { "content-type": "image/png", "cache-control": `public, max-age=${p.live ? 3600 : 60}` },
	});
	ctx.waitUntil(caches.default.put(key, response.clone()));
	return response;
}

/** Rewrites the tags of apps/web/index.html that link previews and search engines read. */
function head(response: Response, p: Page): Response {
	const content: Record<string, string> = {
		'meta[name="description"]': p.description,
		'meta[property="og:title"]': p.title,
		'meta[property="og:description"]': p.description,
		'meta[property="og:url"]': SITE + p.path,
		'meta[property="og:image"]': SITE + p.og,
		// X reads its own tags first, so they follow the page too
		'meta[name="twitter:title"]': p.title,
		'meta[name="twitter:description"]': p.description,
		'meta[name="twitter:image"]': SITE + p.og,
		'meta[property="og:image:alt"]': `${p.image.label ? `${p.image.label}: ` : ""}${p.image.title.replace(/\.$/, "")}. ${p.image.line} Independent, not affiliated with Aave Labs or the Aave DAO.`,
	};
	let canonical = false;
	let rewriter = new HTMLRewriter()
		.on("title", { element: (e) => void e.setInnerContent(p.title) })
		.on('link[rel="canonical"]', { element: (e) => void ((canonical = true), e.setAttribute("href", SITE + p.path)) })
		.on("head", { element: (e) => void e.onEndTag((end) => void (canonical || end.before(`<link rel="canonical" href="${esc(SITE + p.path)}" />`, { html: true }))) });
	for (const [selector, value] of Object.entries(content)) {
		rewriter = rewriter.on(selector, { element: (e) => void e.setAttribute("content", value) });
	}
	return rewriter.transform(response);
}

// shortcut: no rate limit here, and /og/wallet/<any address>.png costs four API reads and a render; add a Workers rate-limit binding per IP before the site is announced.
