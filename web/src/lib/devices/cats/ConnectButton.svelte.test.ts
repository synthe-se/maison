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
const name = m.device_connection({ name: 'Distributeur' });

describe('ConnectButton', () => {
	afterEach(() => {
		forgetAll();
		ui.toasts = [];
	});

	it('shows the state, not the action: a connected device is pressed, its icon the wifi', async () => {
		await render(ConnectButton, { device: feeder });
		await expect.element(page.getByRole('button', { name })).toHaveAttribute('aria-pressed', 'true');
		const { rerender } = await render(ConnectButton, { device: { ...feeder, id: 'f2', connected: false } });
		void rerender;
		await expect.element(page.getByRole('button', { name }).nth(1)).toHaveAttribute('aria-pressed', 'false');
	});

	it('disconnects a connected device, busy while it travels (keeping the focus), then says so', async () => {
		const answer = deferred();
		const api = stubApi({ 'POST /devices/f1/disconnect': () => answer.promise.then(() => ({ success: true })) });
		await render(ConnectButton, { device: feeder });
		const button = page.getByRole('button', { name });
		await button.click();
		await expect.element(button).toHaveAttribute('aria-busy', 'true');
		await expect.element(button).toHaveAttribute('aria-disabled', 'true');
		await expect.element(button).toHaveFocus();
		answer.resolve(undefined);
		await expect.element(button).not.toHaveAttribute('aria-busy');
		expect(api.sent('POST', '/devices/f1/disconnect')).toHaveLength(1);
		expect(ui.toasts.map((t) => t.text)).toContain(m.device_disconnected_description({ name: 'Distributeur' }));
	});

	it('connects a disconnected device', async () => {
		const api = stubApi({ 'POST /devices/f1/connect': { success: true } });
		await render(ConnectButton, { device: { ...feeder, connected: false } });
		await page.getByRole('button', { name }).click();
		await expect.poll(() => api.sent('POST', '/devices/f1/connect')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.device_connection_initiated_description({ name: 'Distributeur' }));
	});

	it('tells a refusal and becomes usable again', async () => {
		stubApi({ 'POST /devices/f1/connect': new Response(JSON.stringify({ error: 'Tuya timeout' }), { status: 504 }) });
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		await render(ConnectButton, { device: { ...feeder, connected: false } });
		const button = page.getByRole('button', { name });
		await button.click();
		await expect.poll(() => fail).toHaveBeenCalledOnce();
		await expect.element(button).not.toHaveAttribute('aria-disabled');
	});
});
