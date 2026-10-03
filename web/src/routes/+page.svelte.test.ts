import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { tuyaDevice } from '#lib/test/tuya.ts';
import Dashboard from './+page.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));

describe('dashboard', () => {
	afterEach(() => forgetAll());

	it('is titled « Accueil », its groups in a fixed order', async () => {
		stubApi({
			'/devices': { success: true, devices: [tuyaDevice()], total: 1, message: '' },
			'/ir/keymap': { success: true, keymap: {} },
			'/tempo': () => new Promise(() => {})
		});
		await render(Dashboard);
		await expect.element(page.getByRole('heading', { level: 1, name: m.nav_home() })).toBeVisible();
		await expect.poll(() => document.title).toContain(m.nav_home());
		// the owner's order: Tempo, the lights, the cats, the AC, the shutters, the plugs, the
		// rabbit, then the remotes at the bottom (TV and box last)
		const h2 = (name: string) => page.getByRole('heading', { level: 2, name, exact: true });
		const order = [m.nav_tempo(), m.dashboard_cats_title(), m.climate_dashboard_title(), m.shutters_title(), m.meross_title(), m.remote_title(), m.tv_title(), m.android_tv_title()];
		for (const name of order) await expect.element(h2(name)).toBeInTheDocument();
		const els = order.map((name) => h2(name).element());
		for (let i = 1; i < els.length; i++) expect(els[i - 1].compareDocumentPosition(els[i]) & Node.DOCUMENT_POSITION_FOLLOWING, order[i]).toBeTruthy();
		await expect.element(page.getByRole('article', { name: 'Distributeur' })).toBeVisible();
	});

	it('leaves out what the house does not have (no rabbit)', async () => {
		const api = stubApi({ '/nabaztag': { success: true, config: { host: null, tempoEnabled: false }, reachable: false } });
		await render(Dashboard);
		await expect.poll(() => api.calls.map((c) => c.path)).toEqual(expect.arrayContaining(['/nabaztag']));
		await expect.element(page.getByRole('heading', { name: m.nabaztag_name() })).not.toBeInTheDocument();
	});
});
