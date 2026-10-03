import { tuyaDevice } from '#lib/test/tuya.ts';
import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { Device } from '#lib/devices/cats/api.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import CatsGroup from './CatsGroup.svelte';

const feeder = tuyaDevice();
const fountain: Device = { id: 'w1', name: 'Fontaine', type: 'fountain', connected: false };
const list = (devices: Device[]) => ({ success: true, devices, total: devices.length, message: '' });
const tile = (name: string) => page.getByRole('article', { name });

describe('CatsGroup', () => {
	afterEach(() => {
		ui.toasts = [];
	});

	it('says it is loading, without a jump when the tiles come', async () => {
		stubApi({ '/devices': () => new Promise(() => {}) });
		await render(CatsGroup);
		// no live region of its own: a load is never announced (§ 4); skeletons hold the place
		await expect.element(page.getByText(m.common_loading())).toBeInTheDocument();
		expect(document.querySelectorAll('.skeleton').length).toBeGreaterThan(0);
		await expect.element(page.getByRole('heading', { name: m.dashboard_cats_title() })).toBeVisible();
	});

	it('shows each device as a tile leading to its page, its state in words', async () => {
		stubApi({ '/devices': list([feeder, fountain]) });
		await render(CatsGroup);
		await expect.element(tile('Distributeur')).toMatchTextContent(m.device_online());
		await expect.element(tile('Fontaine')).toMatchTextContent(m.device_offline());
		await expect.element(page.getByRole('link', { name: 'Distributeur' })).toHaveAttribute('href', '/device/f1');
		await expect.element(page.getByText(m.dashboard_device_count({ count: 2 }))).toBeVisible();
		// no icon that looks like a signal indicator and cuts the connection
		await expect.element(tile('Fontaine').getByRole('button')).not.toBeInTheDocument();
		// only a feeder serves
		await expect.element(tile('Fontaine').getByRole('button', { name: m.feeder_distribute({ count: 1 }) })).not.toBeInTheDocument();
	});

	it('serves one portion from the tile, busy meanwhile, then says when', async () => {
		const answer = deferred();
		const api = stubApi({ '/devices': list([feeder]), 'POST /devices/f1/feeder/feed': () => answer.promise });
		await render(CatsGroup);
		const give = tile('Distributeur').getByRole('button', { name: m.feeder_distribute({ count: 1 }) });
		await give.click();
		await expect.element(give).toBeDisabled();
		await expect.element(give).toHaveAttribute('aria-busy', 'true');
		answer.resolve({ success: true });
		await expect.element(give).toBeEnabled();
		await expect.element(tile('Distributeur')).toMatchTextContent(m.feeder_served_at({ time: '' }).trim());
		expect(api.sent('POST', '/devices/f1/feeder/feed')[0].body).toEqual({ portion: 1 });
	});

	it('cannot serve from a feeder that is offline: the button stays, unavailable, described by the tile’s state line', async () => {
		const api = stubApi({ '/devices': list([{ ...feeder, connected: false }]) });
		await render(CatsGroup);
		const give = page.getByRole('button', { name: m.feeder_distribute({ count: 1 }) });
		await expect.element(give).toHaveAttribute('aria-disabled', 'true');
		await expect.element(give).toHaveAccessibleDescription(m.device_offline());
		(give.element() as HTMLElement).click();
		expect(api.sent('POST', '/devices/f1/feeder/feed')).toHaveLength(0);
	});

	it('line 2 says what matters, in the warning style; « En ligne » only when nothing to say', async () => {
		const litter: Device = { id: 'l1', name: 'Litière', type: 'litter-box', connected: true };
		const water: Device = { ...fountain, connected: true };
		// a feeder of its own: another test served from f1 (the time it served is kept)
		const fed = tuyaDevice({ id: 'f2' });
		const api = stubApi({
			'/devices': list([fed, water, litter]),
			'/devices/f2/feeder/status': {
				success: true,
				device: fed,
				parsedStatus: { system: { faultStatus: false, poweredBy: 'AC Power' } },
				message: ''
			},
			'/devices/w1/fountain/status': { success: true, device: water, parsedStatus: { waterLevel: 'low' }, message: '' },
			'/devices/l1/litter-box/status': {
				success: true,
				device: litter,
				parsedStatus: { sensors: { litterLevel: 'half' }, system: { state: 'satnd_by' } },
				message: ''
			}
		});
		await render(CatsGroup);
		await expect.element(tile('Fontaine')).toMatchTextContent(m.cats_water_low());
		await expect.element(tile('Fontaine').getByText(m.cats_water_low())).toHaveClass('warn');
		await expect.element(tile('Litière')).toMatchTextContent(m.cats_litter_half());
		await expect.element(tile('Distributeur')).toMatchTextContent(m.device_online());
		// once refilled: clean, said plainly
		api.routes['/devices/l1/litter-box/status'] = {
			success: true,
			device: litter,
			parsedStatus: { sensors: { litterLevel: 'full' }, system: { state: 'satnd_by' } },
			message: ''
		};
		await (await import('#lib/live.svelte.ts')).refresh('tuya:');
		await expect.element(tile('Litière').getByText(m.cats_clean())).not.toHaveClass('warn');
	});

	it('the local connection is a labelled switch per device in the group’s menu', async () => {
		const api = stubApi({ '/devices': list([feeder, fountain]), 'POST /devices/w1/connect': { success: true } });
		await render(CatsGroup);
		await page.getByRole('button', { name: m.dashboard_cats_actions() }).click();
		await expect.element(page.getByRole('menuitemcheckbox', { name: 'Distributeur' })).toHaveAttribute('aria-checked', 'true');
		const off = page.getByRole('menuitemcheckbox', { name: 'Fontaine' });
		await expect.element(off).toHaveAttribute('aria-checked', 'false');
		await off.click();
		await expect.poll(() => api.sent('POST', '/devices/w1/connect')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.device_connection_initiated_description({ name: 'Fontaine' }));
	});

	it('says when there is no device, and how to add one', async () => {
		stubApi({ '/devices': list([]) });
		await render(CatsGroup);
		await expect.element(page.getByText(m.dashboard_no_devices())).toBeVisible();
		await expect.element(page.getByText(m.dashboard_no_devices_hint())).toBeVisible();
	});

	it('says when the list cannot be read, and tries again on demand', async () => {
		const api = stubApi({ '/devices': new Response('{"error":"down"}', { status: 500 }) });
		await render(CatsGroup);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		api.routes['/devices'] = list([feeder]);
		await page.getByRole('button', { name: m.common_retry() }).click();
		await expect.element(tile('Distributeur')).toBeVisible();
	});

	it('connects or disconnects everyone from its menu, says so and reads the list again', async () => {
		const api = stubApi({
			'/devices': list([feeder]),
			'POST /devices/connect': { success: true },
			'POST /devices/disconnect': { success: true }
		});
		await render(CatsGroup);
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
