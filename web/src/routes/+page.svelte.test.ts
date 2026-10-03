import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { stubApi } from '#lib/test/api.ts';
import { tuyaDevice } from '#lib/test/tuya.ts';
import Dashboard from './+page.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));

describe('dashboard', () => {
	it('is titled « Accueil », its groups in a fixed order', async () => {
		stubApi({
			'/devices': { success: true, devices: [tuyaDevice()], total: 1, message: '' },
			'/tempo': () => new Promise(() => {})
		});
		await render(Dashboard);
		await expect.element(page.getByRole('heading', { level: 1, name: m.nav_home() })).toBeVisible();
		await expect.poll(() => document.title).toContain(m.nav_home());
		// « Maintenant » first, then the owner's order: Tempo, the lights, the cats, the AC, the
		// shutters, the plugs, the rabbit, then « Télé » (the TV and the box) at the bottom
		const h2 = (name: string) => page.getByRole('heading', { level: 2, name, exact: true });
		const order = [
			m.now_title(),
			m.nav_tempo(),
			m.zigbee_lamps_title(),
			m.dashboard_cats_title(),
			m.climate_dashboard_title(),
			m.shutters_title(),
			m.meross_title(),
			m.tv_group_title()
		];
		for (const name of order) await expect.element(h2(name)).toBeInTheDocument();
		const els = order.map((name) => h2(name).element());
		for (let i = 1; i < els.length; i++)
			expect(els[i - 1].compareDocumentPosition(els[i]) & Node.DOCUMENT_POSITION_FOLLOWING, order[i]).toBeTruthy();
		await expect.element(page.getByRole('article', { name: 'Distributeur' })).toBeVisible();
		// the remote's keymap has its own destination: not on the home
		for (const name of [m.remote_title(), m.tv_title(), m.android_tv_title()]) await expect.element(h2(name)).not.toBeInTheDocument();
		expect(document.querySelector('main a[href="/remote"], .dashboard a[href="/remote"]')).toBeNull();
	});

	it('leaves out what the house does not have (no rabbit)', async () => {
		const api = stubApi({ '/nabaztag': { success: true, config: { host: null, tempoEnabled: false }, reachable: false } });
		await render(Dashboard);
		await expect.poll(() => api.calls.map((c) => c.path)).toEqual(expect.arrayContaining(['/nabaztag']));
		await expect.element(page.getByRole('heading', { name: m.nabaztag_name() })).not.toBeInTheDocument();
	});
});
