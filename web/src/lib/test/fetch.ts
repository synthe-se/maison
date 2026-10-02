// Shared fetch stubs for tests: no test ever reaches a network.

import { vi } from 'vitest';

export type FetchCall = { url: string; init?: RequestInit };

/** A JSON response, as the backend sends them. */
export const json = (body: unknown, status = 200) =>
	new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

/** Stubs `fetch` with a handler (url, init) → Response; returns the recorded calls. */
export function stubFetch(handler: (url: string, init?: RequestInit) => Response | Promise<Response>) {
	const calls: FetchCall[] = [];
	vi.stubGlobal(
		'fetch',
		vi.fn(async (url: string, init?: RequestInit) => {
			calls.push({ url, init });
			return handler(url, init);
		})
	);
	return calls;
}

/** Stubs `fetch` with one scripted response per call, in order; an extra call fails the test. */
export function scriptFetch(...responses: (Response | Error)[]) {
	return stubFetch((url) => {
		const next = responses.shift();
		if (!next) throw new Error(`unexpected call to ${url}`);
		if (next instanceof Error) throw next;
		return next;
	});
}

/** The JSON body a recorded call sent. */
export const sentBody = (call: FetchCall) => JSON.parse(String(call.init?.body ?? 'null'));
