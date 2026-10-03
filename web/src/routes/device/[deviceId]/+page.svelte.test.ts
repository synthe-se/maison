import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { Device } from '#lib/devices/cats/api.ts';
import { stubApi } from '#lib/test/api.ts';
import { tuyaDevice } from '#lib/test/tuya.ts';
import DevicePage from './+page.svelte';

// the route's parameters, as SvelteKit would give them
const route = vi.hoisted(() => ({ params: { deviceId: 'f1' } as { deviceId?: string } }));
vi.mock('$app/state', () => ({ page: route }));

const devices: Device[] = [
	tuyaDevice({ productName: 'Petlibro Air', ip: '192.168.1.20', version: '3.3' }),
	tuyaDevice({ id: 'w1', name: 'Fontaine', type: 'fountain', ip: '192.168.1.21', connected: false }),
	tuyaDevice({ id: 'l1', name: 'Litière', type: 'litter-box' }),
	tuyaDevice({ id: 'u1', name: 'Mystère', type: 'unknown' })
];
const open = async (id: string | undefined, list: unknown = { success: true, devices, total: devices.length, message: '' }) => {
	route.params = { deviceId: id };
	const api = stubApi({ '/devices': list });
	await render(DevicePage);
	return api;
};
const title = (name: string) => page.getByRole('heading', { level: 1, name });

describe('device page', () => {
	it('says it is loading, with the way back', async () => {
		await open('f1', () => new Promise(() => {}));
		await expect.element(title(m.common_loading())).toBeVisible();
		await expect.element(page.getByRole('link', { name: m.back_home() })).toHaveAttribute('href', '/');
	});

	it('says when the device does not exist', async () => {
		await open('nope');
		await expect.element(title(m.device_not_found())).toBeVisible();
		await expect.element(page.getByText(m.device_not_found_description())).toBeVisible();
	});

	it('names a feeder, says it is online and what it is, and shows its controls', async () => {
		await open('f1');
		await expect.element(title('Distributeur')).toBeVisible();
		await expect
			.element(page.getByText(`${m.device_online()} · Petlibro Air · 192.168.1.20 · ${m.device_version({ version: '3.3' })}`))
			.toBeVisible();
		await expect.element(page.getByRole('switch', { name: m.device_local_connection() })).toBeChecked();
		await expect.element(page.getByRole('heading', { name: m.feeder_manual_distribution() })).toBeVisible();
	});

	it('names an offline fountain by its type, and shows its controls', async () => {
		await open('w1');
		await expect.element(title('Fontaine')).toBeVisible();
		await expect.element(page.getByText(`${m.device_offline()} · ${m.device_types_fountain()} · 192.168.1.21`)).toBeVisible();
		await expect.element(page.getByRole('switch', { name: m.device_local_connection() })).not.toBeChecked();
	});

	it('the local connection is a labelled switch: switching it connects the device and says so', async () => {
		const api = await open('w1');
		api.routes['POST /devices/w1/connect'] = { success: true };
		await page.getByRole('switch', { name: m.device_local_connection() }).click();
		await expect.poll(() => api.sent('POST', '/devices/w1/connect')).toHaveLength(1);
	});

	it('/device/feeder (the app shortcut) opens the first feeder', async () => {
		await open('feeder');
		await expect.element(title('Distributeur')).toBeVisible();
	});

	it('shows a litter box’s controls', async () => {
		const api = await open('l1');
		await expect.element(title('Litière')).toBeVisible();
		await expect.poll(() => api.calls.map((c) => c.path)).toContain('/devices/l1/litter-box/status');
	});

	it('says there is nothing to control on a device of unknown type', async () => {
		await open('u1');
		await expect.element(page.getByText(m.device_unknown_type())).toBeVisible();
	});

	it('takes a missing parameter for a device that does not exist', async () => {
		await open(undefined);
		await expect.element(title(m.device_not_found())).toBeVisible();
	});
});
