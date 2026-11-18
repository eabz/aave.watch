import { SELF } from "cloudflare:test";
import { expect, it } from "vitest";

it("answers /v1/* from the Worker, not from static assets", async () => {
	const response = await SELF.fetch("https://example.com/v1/anything");
	// Either the API answers through the tunnel (404, no such endpoint) or it is not reachable (503,
	// 502). What must never happen is the single-page app's index.html with a 200.
	expect(response.status).toBeGreaterThanOrEqual(400);
	expect(response.headers.get("content-type")).toContain("application/json");
	expect(await response.json()).toHaveProperty("error");
});

it("refuses writes", async () => {
	const response = await SELF.fetch("https://example.com/v1/positions", { method: "POST" });
	expect(response.status).toBe(405);
});
