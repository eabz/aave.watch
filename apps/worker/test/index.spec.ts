import { SELF } from "cloudflare:test";
import { expect, it } from "vitest";

it("answers /v1/* from the Worker, not from static assets", async () => {
	const response = await SELF.fetch("https://example.com/v1/anything");
	expect(response.status).toBe(501);
});
