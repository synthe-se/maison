import { tuyaDevice } from '#lib/test/tuya.ts';
import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { Device } from '#lib/api.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import CatDevices from './CatDevices.svelte';

const feeder = tuyaDevice();
const fountain: Device = { id: 'w1', name: 'Fontaine', type: 'fountain', connected: false };
const list = (devices: Device[]) => ({ success: true, devices, total: devices.length, message: '' });
const tile = (name: string) => page.getByRole('article', { name });

describe('CatDevices', () => {
	afterEach(() => {
		forgetAll();
		ui.toasts = [];
	});

	it('says it is loading, without a jump when the tiles come', async () => {
		stubApi({ '/devices': () => new Promise(() => {}) });
		await render(CatDevices);
		await expect.element(page.getByRole('status')).toHaveTextContent(m.common_loading());
		await expect.element(page.getByRole('heading', { name: m.dashboard_cats_title() })).toBeVisible();
	});

	it('shows each device as a tile leading to its page, its state in words', async () => {
		stubApi({ '/devices': list([feeder, fountain]) });
		await render(CatDevices);
		await expect.element(tile('Distributeur')).toMatchTextContent(m.device_online());
		await expect.element(tile('Fontaine')).toMatchTextContent(m.device_offline());
		await expect.element(page.getByRole('link', { name: 'Distributeur' })).toHaveAttribute('href', '/device/f1');
		await expect.element(page.getByText(m.dashboard_device_count({ count: 2 }))).toBeVisible();
		await expect.element(tile('Fontaine').getByRole('button', { name: m.device_connect({ name: 'Fontaine' }) })).toBeVisible();
		// only a feeder serves
		await expect.element(tile('Fontaine').getByRole('button', { name: m.feeder_distribute({ count: 1 }) })).not.toBeInTheDocument();
	});

	it('serves one portion from the tile, busy meanwhile, then says when', async () => {
		const answer = deferred();
		const api = stubApi({ '/devices': list([feeder]), 'POST /devices/f1/feeder/feed': () => answer.promise });
		await render(CatDevices);
		const give = tile('Distributeur').getByRole('button', { name: m.feeder_distribute({ count: 1 }) });
		await give.click();
		await expect.element(give).toBeDisabled();
		await expect.element(give).toHaveAttribute('aria-busy', 'true');
		answer.resolve({ success: true });
		await expect.element(give).toBeEnabled();
		await expect.element(tile('Distributeur')).toMatchTextContent(m.feeder_served_at({ time: '' }).trim());
		expect(api.sent('POST', '/devices/f1/feeder/feed')[0].body).toEqual({ portion: 1 });
	});

	it('cannot serve from a feeder that is offline', async () => {
		stubApi({ '/devices': list([{ ...feeder, connected: false }]) });
		await render(CatDevices);
		await expect.element(page.getByRole('button', { name: m.feeder_distribute({ count: 1 }) })).toBeDisabled();
	});

	it('says when there is no device, and how to add one', async () => {
		stubApi({ '/devices': list([]) });
		await render(CatDevices);
		await expect.element(page.getByText(m.dashboard_no_devices())).toBeVisible();
		await expect.element(page.getByText(m.dashboard_no_devices_hint())).toBeVisible();
	});

	it('says when the list cannot be read, and tries again on demand', async () => {
		const api = stubApi({ '/devices': new Response('{"error":"down"}', { status: 500 }) });
		await render(CatDevices);
		await expect.element(page.getByText(m.dashboard_loading_error())).toBeVisible();
		api.routes['/devices'] = list([feeder]);
		await page.getByRole('button', { name: m.common_retry() }).click();
		await expect.element(tile('Distributeur')).toBeVisible();
	});

	it('connects or disconnects everyone from its menu, says so and reads the list again', async () => {
		const api = stubApi({ '/devices': list([feeder]), 'POST /devices/connect': { success: true }, 'POST /devices/disconnect': { success: true } });
		await render(CatDevices);
		await expect.element(tile('Distributeur')).toBeVisible();
		await page.getByRole('button', { name: m.dashboard_cats_actions() }).click();
		await page.getByRole('menuitem', { name: m.dashboard_connect_all() }).click();
		await expect.poll(() => api.sent('POST', '/devices/connect')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.dashboard_global_connection_description());
		await expect.poll(() => api.sent('GET', '/devices')).toHaveLength(2);
		await page.getByRole('button', { name: m.dashboard_cats_actions() }).click();
		await page.getByRole('menuitem', { name: m.dashboard_disconnect_all() }).click();
		await expect.poll(() => api.sent('POST', '/devices/disconnect')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.dashboard_global_disconnection_description());
		await page.getByRole('button', { name: m.dashboard_cats_actions() }).click();
		await page.getByRole('menuitem', { name: m.common_refresh() }).click();
		await expect.poll(() => api.sent('GET', '/devices')).toHaveLength(4);
	});
});
