import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import Dashboard from './+page.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));

describe('dashboard', () => {
	afterEach(() => forgetAll());

	it('is titled « Accueil », its groups in a fixed order', async () => {
		stubApi({
			'/devices': { success: true, devices: [{ id: 'f1', name: 'Distributeur', type: 'feeder', connected: true }], total: 1, message: '' },
			'/ir/keymap': { success: true, keymap: {} },
			'/tempo': () => new Promise(() => {})
		});
		await render(Dashboard);
		await expect.element(page.getByRole('heading', { level: 1, name: m.nav_home() })).toBeVisible();
		await expect.poll(() => document.title).toContain(m.nav_home());
		const cats = page.getByRole('heading', { level: 2, name: m.dashboard_cats_title() });
		const tempo = page.getByRole('heading', { level: 2, name: m.tempo_title() });
		const remote = page.getByRole('heading', { level: 2, name: m.remote_title() });
		await expect.element(cats).toBeVisible();
		await expect.element(tempo).toBeVisible();
		await expect.element(remote).toBeVisible();
		const order = [cats, tempo, remote].map((h) => h.element());
		expect(order[0].compareDocumentPosition(order[1]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		expect(order[1].compareDocumentPosition(order[2]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		await expect.element(page.getByRole('article', { name: 'Distributeur' })).toBeVisible();
	});

	it('leaves out what the house does not have (no Tempo, no rabbit)', async () => {
		const api = stubApi({ '/tempo': { success: false, message: '' }, '/nabaztag': { success: true, config: { host: null, tempoEnabled: false }, reachable: false, status: null } });
		await render(Dashboard);
		await expect.poll(() => api.calls.map((c) => c.path)).toEqual(expect.arrayContaining(['/tempo', '/nabaztag']));
		await expect.element(page.getByRole('heading', { name: m.tempo_title() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('heading', { name: m.nabaztag_name() })).not.toBeInTheDocument();
	});
});
