import installScript from "./install.sh";

export default {
	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);
		if (url.pathname === "/meno/install.sh" || url.pathname === "/meno/install") {
			return new Response(installScript, {
				status: 200,
				headers: {
					"Content-Type": "text/plain; charset=utf-8",
					"Cache-Control": "public, max-age=600",
					"X-Content-Type-Options": "nosniff",
				},
			});
		}
		return new Response("Not Found", { status: 404 });
	},
};
