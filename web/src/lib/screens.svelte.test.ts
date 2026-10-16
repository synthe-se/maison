import { afterEach, describe, expect, it, vi } from 'vitest';
import { locale, switchLocale } from '#lib/i18n.svelte.ts';
import { json, scriptFetch } from '#lib/test/fetch.ts';
import { ui } from '#lib/ui.svelte.ts';
import { screensApi, takeAddress } from './screens.ts';

/** An address as the frame opens it, and the history its fragment is wiped from. */
function at(hash: string) {
	const location = { pathname: '/salon', search: '?x=1', hash } as Location;
	const history = { state: null, replaceState: vi.fn() } as unknown as History;
	return [location, history] as const;
}

describe('takeAddress', () => {
	afterEach(() => {
		switchLocale('fr');
		ui.setTheme('system');
	});

	it('takes the language its address names, and wipes it from the address', () => {
		switchLocale('en');
		const [location, history] = at('#lang=fr');
		takeAddress(location, history);
		expect(locale()).toBe('fr');
		expect(document.documentElement.lang).toBe('fr');
		expect(history.replaceState).toHaveBeenCalledWith(null, '', '/salon?x=1');
	});

	it('takes the theme its address names (a wall screen follows the sun), beside the language', () => {
		const [location, history] = at('#lang=fr&theme=sun');
		takeAddress(location, history);
		expect(ui.theme).toBe('sun');
		expect(history.replaceState).toHaveBeenCalledOnce();
		takeAddress(...at('#theme=dusk'));
		expect(ui.theme).toBe('sun');
	});

	it('leaves the language as it is for one it does not have, and an address without one', () => {
		switchLocale('en');
		const [location, history] = at('#lang=xx');
		takeAddress(location, history);
		expect(locale()).toBe('en');
		expect(history.replaceState).toHaveBeenCalledOnce();
		const [plain, untouched] = at('');
		takeAddress(plain, untouched);
		expect(untouched.replaceState).not.toHaveBeenCalled();
	});
});

describe('screensApi', () => {
	it('claims with the secret in the body, never in an address', async () => {
		const calls = scriptFetch(json({ success: true, paired: false }));
		await screensApi.claim('s3cret');
		expect(calls[0].url).toBe('/api/screens/pairing/claim');
		expect(JSON.parse(String(calls[0].init?.body))).toEqual({ secret: 's3cret' });
	});

	it('lists the screens an admin sees', async () => {
		const screen = { id: 'a1', name: 'Salon', createdMs: 1, lastUsedMs: null };
		scriptFetch(json({ success: true, screens: [screen] }));
		expect(await screensApi.list()).toEqual([screen]);
	});
});
