import { afterEach, describe, expect, it, vi } from 'vitest';
import { api, useToken } from '#lib/api.ts';
import { json, scriptFetch } from '#lib/test/fetch.ts';
import { claimScreen } from './screen.ts';

const KEY = 'maison-screen-token';
const TOKEN = `maison_${'a'.repeat(43)}`;

/** An address as the frame opens it, and the history its fragment is wiped from. */
function at(hash: string) {
	const location = { pathname: '/salon', search: '?x=1', hash } as Location;
	const history = { state: null, replaceState: vi.fn() } as unknown as History;
	return [location, history] as const;
}

describe('claimScreen', () => {
	afterEach(() => {
		localStorage.removeItem(KEY);
		useToken(null);
	});

	it('is no screen without a token, given or kept', () => {
		const [location, history] = at('');
		expect(claimScreen(location, history)).toBe(false);
		expect(history.replaceState).not.toHaveBeenCalled();
	});

	it('takes the token from the fragment, keeps it, wipes it from the address, and sends it', async () => {
		const [location, history] = at(`#token=${TOKEN}`);
		expect(claimScreen(location, history)).toBe(true);
		expect(history.replaceState).toHaveBeenCalledWith(null, '', '/salon?x=1');
		expect(localStorage.getItem(KEY)).toBe(TOKEN);
		const calls = scriptFetch(json({}));
		await api('/scenes');
		expect(new Headers(calls[0].init?.headers).get('Authorization')).toBe(`Bearer ${TOKEN}`);
		// opened again without it: still the screen
		expect(claimScreen(...at(''))).toBe(true);
	});

	it('a new token replaces the kept one', () => {
		localStorage.setItem(KEY, TOKEN);
		const next = `maison_${'b'.repeat(43)}`;
		expect(claimScreen(...at(`#token=${next}`))).toBe(true);
		expect(localStorage.getItem(KEY)).toBe(next);
	});

	it('refuses what is not a token, and still wipes it from the address', () => {
		const [location, history] = at('#token=maison_short');
		expect(claimScreen(location, history)).toBe(false);
		expect(history.replaceState).toHaveBeenCalledOnce();
		expect(localStorage.getItem(KEY)).toBeNull();
		// a kept value that is no token is no screen either
		localStorage.setItem(KEY, 'nope');
		expect(claimScreen(...at(''))).toBe(false);
	});
});
