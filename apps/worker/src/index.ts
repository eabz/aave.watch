// API proxy and SSE relay for /v1/*. Everything else is a static asset (see wrangler.jsonc).
export default {
	async fetch(): Promise<Response> {
		return new Response("not implemented", { status: 501 });
	},
} satisfies ExportedHandler<Env>;
