import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { json, stubFetch } from '#lib/test/fetch.ts';
import ZigbeePairing from './ZigbeePairing.svelte';

afterEach(() => {
	vi.useRealTimers();
	ui.toasts = [];
});

const pairing = (active: boolean, remainingSeconds = 0, message?: string) => ({
	success: true,
	pairing: { active, remainingSeconds, permitJoinSeconds: 120, message },
	message: ''
});

describe('ZigbeePairing', () => {
	it('opening the network: one POST, the countdown shown and said once', async () => {
		const say = vi.spyOn(ui, 'say');
		const calls = stubFetch((url) => {
			if (url === '/api/zigbee/lamps/pairing/status') return json(pairing(false));
			if (url === '/api/zigbee/lamps/pairing/start') return json(pairing(true, 120));
			if (url === '/api/zigbee/lamps') return json({ success: true, lamps: [] });
			throw new Error(url);
		});
		await render(ZigbeePairing);
		await expect.element(page.getByText(m.zigbee_lamps_pairing_inactive())).toBeVisible();
		await page.getByRole('button', { name: m.zigbee_lamps_start_pairing() }).click();
		await expect.element(page.getByText(m.zigbee_lamps_pairing_active({ count: 120 }))).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.zigbee_lamps_stop_pairing() })).toBeVisible();
		const start = calls.filter((c) => c.url === '/api/zigbee/lamps/pairing/start');
		expect(start).toHaveLength(1);
		expect(start[0].init?.method).toBe('POST');
		expect(say).toHaveBeenCalledWith(m.zigbee_lamps_pairing_active({ count: 120 }));
	});

	it('closing it: POST stop, « Appairage fermé » said', async () => {
		const say = vi.spyOn(ui, 'say');
		const calls = stubFetch((url) => {
			if (url === '/api/zigbee/lamps/pairing/status') return json(pairing(true, 30, 'Coordinator open'));
			if (url === '/api/zigbee/lamps/pairing/stop') return json(pairing(false));
			throw new Error(url);
		});
		await render(ZigbeePairing);
		// the server's own words are never shown
		await expect.element(page.getByText(m.zigbee_lamps_pairing_active({ count: 30 }))).toBeVisible();
		expect(page.getByText('Coordinator open').query()).toBeNull();
		await page.getByRole('button', { name: m.zigbee_lamps_stop_pairing() }).click();
		await expect.element(page.getByRole('button', { name: m.zigbee_lamps_start_pairing() })).toBeVisible();
		expect(calls.filter((c) => c.url === '/api/zigbee/lamps/pairing/stop' && c.init?.method === 'POST')).toHaveLength(1);
		expect(say).toHaveBeenCalledWith(m.zigbee_lamps_pairing_inactive());
	});

	it('counts the window down itself, reading it only every 10 s, and once more when it ends', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'Date'] });
		let active = true;
		const calls = stubFetch(() => json(pairing(active, active ? 12 : 0)));
		await render(ZigbeePairing);
		const reads = () => calls.length;
		// each read is answered by the browser's own fetch machinery: wait for it between ticks
		await expect.poll(reads).toBe(1);
		await expect.element(page.getByText(m.zigbee_lamps_pairing_active({ count: 12 }))).toBeVisible();
		await vi.advanceTimersByTimeAsync(3000);
		await expect.element(page.getByText(m.zigbee_lamps_pairing_active({ count: 9 }))).toBeVisible();
		expect(reads()).toBe(1);
		await vi.advanceTimersByTimeAsync(7000);
		await expect.poll(reads).toBe(2);
		// the window ends: one read, which says it closed
		active = false;
		await vi.advanceTimersByTimeAsync(12000);
		await expect.element(page.getByText(m.zigbee_lamps_pairing_inactive())).toBeVisible();
		expect(reads()).toBeLessThanOrEqual(4);
	});

	it('Touchlink: one POST and a toast', async () => {
		const calls = stubFetch((url) => {
			if (url === '/api/zigbee/lamps/pairing/status') return json(pairing(false));
			if (url === '/api/zigbee/lamps/pairing/touchlink') return json({ success: true, message: '' });
			throw new Error(url);
		});
		await render(ZigbeePairing);
		await page.getByRole('button', { name: m.zigbee_lamps_touchlink_scan() }).click();
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.zigbee_lamps_touchlink_started());
		expect(calls.find((c) => c.url.endsWith('/touchlink'))?.init?.method).toBe('POST');
	});
});
