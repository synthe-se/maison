// A stand-in backend for component tests: replies by route, records what was sent. Built on
// stubFetch (fetch.ts), so no test ever reaches a network.

import { json, stubFetch } from './fetch.ts';

/** One request as the backend would see it: path without `/api`, query included, JSON body parsed. */
export type ApiCall = { method: string; path: string; body: unknown };

/**
 * What a route answers: a body (sent as JSON, 200), a Response (cloned, so it can be reused),
 * an Error (thrown: no answer at all), or a function of the call returning one of these (it
 * may return a promise that never settles: a request still travelling).
 */
export type Reply = unknown;

/**
 * Stubs `fetch` with routes keyed « METHOD /path » or « /path » (any method). An unknown route
 * answers `otherwise` when given (a backend that says « ok » to every command), else 404 with an
 * error message naming it. The routes object stays live: a test may change a reply between two
 * requests (`api.routes['/x'] = …`).
 */
export function stubApi(routes: Record<string, Reply>, otherwise?: Reply) {
	const calls: ApiCall[] = [];
	stubFetch(async (url, init) => {
		const path = url.replace(/^\/api/, '');
		const method = init?.method ?? 'GET';
		const call: ApiCall = { method, path, body: init?.body ? JSON.parse(String(init.body)) : undefined };
		calls.push(call);
		let reply = `${method} ${path}` in routes ? routes[`${method} ${path}`] : (routes[path] ?? otherwise);
		if (reply === undefined) return json({ success: false, error: `no route ${method} ${path}` }, 404);
		if (typeof reply === 'function') reply = await (reply as (c: ApiCall) => unknown)(call);
		if (reply instanceof Error) throw reply;
		if (reply instanceof Response) return reply.clone();
		return json(reply);
	});
	return {
		calls,
		routes,
		/** The calls made with this method to this path (query included). */
		sent: (method: string, path: string) => calls.filter((c) => c.method === method && c.path === path)
	};
}

/** A promise settled when the test says so (a command still travelling). */
export function deferred<T = unknown>() {
	let resolve!: (v: T) => void;
	let reject!: (e: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}
