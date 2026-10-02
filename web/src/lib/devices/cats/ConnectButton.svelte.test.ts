import { tuyaDevice } from '#lib/test/tuya.ts';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import ConnectButton from './ConnectButton.svelte';

const feeder = tuyaDevice();

describe('ConnectButton', () => {
	afterEach(() => {
		forgetAll();
		ui.toasts = [];
	});

	it('disconnects a connected device, busy while it travels, then says so', async () => {
		const answer = deferred();
		const api = stubApi({ 'POST /devices/f1/disconnect': () => answer.promise.then(() => ({ success: true })) });
		await render(ConnectButton, { device: feeder });
		const button = page.getByRole('button', { name: m.device_disconnect({ name: 'Distributeur' }) });
		await button.click();
		await expect.element(button).toBeDisabled();
		await expect.element(button).toHaveAttribute('aria-busy', 'true');
		answer.resolve(undefined);
		await expect.element(button).toBeEnabled();
		expect(api.sent('POST', '/devices/f1/disconnect')).toHaveLength(1);
		expect(ui.toasts.map((t) => t.text)).toContain(m.device_disconnected_description({ name: 'Distributeur' }));
	});

	it('connects a disconnected device', async () => {
		const api = stubApi({ 'POST /devices/f1/connect': { success: true } });
		await render(ConnectButton, { device: { ...feeder, connected: false } });
		await page.getByRole('button', { name: m.device_connect({ name: 'Distributeur' }) }).click();
		await expect.poll(() => api.sent('POST', '/devices/f1/connect')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.device_connection_initiated_description({ name: 'Distributeur' }));
	});

	it('tells a refusal and becomes usable again', async () => {
		stubApi({ 'POST /devices/f1/connect': new Response(JSON.stringify({ error: 'Tuya timeout' }), { status: 504 }) });
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		await render(ConnectButton, { device: { ...feeder, connected: false } });
		const button = page.getByRole('button', { name: m.device_connect({ name: 'Distributeur' }) });
		await button.click();
		await expect.poll(() => fail).toHaveBeenCalledOnce();
		await expect.element(button).toBeEnabled();
	});
});
