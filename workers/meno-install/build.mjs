// Bundle workers/meno-install for API deploys by inlining install.sh
// (same output `wrangler deploy` would produce from src/worker.ts + Text rule).
// Usage: node workers/meno-install/build.mjs [out.js]
// Defaults to stdout.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = join(dirname(fileURLToPath(import.meta.url)), "src");
const script = readFileSync(join(dir, "install.sh"), "utf8");

const out = `const installScript = ${JSON.stringify(script)};

export default {
	async fetch(request) {
		const url = new URL(request.url);
		if (url.pathname === "/meno/install.sh" || url.pathname === "/meno/install") {
			return new Response(installScript, {
				status: 200,
				headers: {
					"Content-Type": "text/plain; charset=utf-8",
					"Cache-Control": "public, max-age=600",
					"X-Content-Type-Options": "nosniff"
				}
			});
		}
		return new Response("Not Found", { status: 404 });
	}
};
`;

const dest = process.argv[2];
if (dest) {
	const { writeFileSync } = await import("node:fs");
	writeFileSync(dest, out);
} else {
	process.stdout.write(out);
}
