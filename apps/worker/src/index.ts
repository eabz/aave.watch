// API proxy and SSE relay for /v1/*. Everything else is a static asset (see wrangler.jsonc).
// The API itself is the Rust daemon behind the tunnel; see docs/api.md.

type Vars = {
	/**
	 * Workers VPC service "aave-watch-api": the API daemon on the box, reached through the Cloudflare
	 * Tunnel "aave-watch-box". There is no public hostname for it. Absent in tests.
	 */
	API?: Fetcher;
};

// Where the API listens on the box, as seen from the tunnel's end.
const API = "http://127.0.0.1:8080";

const fail = (status: number, error: string) =>
	Response.json({ error }, { status, headers: { "cache-control": "no-store" } });

export default {
	async fetch(request, env, ctx): Promise<Response> {
		if (request.method !== "GET" && request.method !== "HEAD") {
			return fail(405, "The API is read-only. Use GET.");
		}
		if (!env.API) {
			return fail(503, "The API is not connected yet.");
		}
		const url = new URL(request.url);
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
	},
} satisfies ExportedHandler<Vars>;

// shortcut: no rate limit here; add a Workers rate-limit binding per IP before the site is announced.
