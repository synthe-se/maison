import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { json, stubFetch } from '#lib/test/fetch.ts';
import { hueLamp } from '#lib/test/lamps.ts';
import HueLamps from './HueLamps.svelte';

afterEach(() => {
	forgetAll();
	ui.toasts = [];
});

const lamps = [hueLamp(), hueLamp({ id: 'hue-2', name: 'Chevet', connected: false, lastSeen: null })];

/** The backend: stats, the list, and a scan that waits for `scan` to be resolved. */
function backend({ disabled = false, scan = Promise.resolve(json({ success: true })) } = {}) {
	return stubFetch((url) => {
		if (url === '/api/hue-lamps/stats') return json({ success: true, total: 2, connected: 1, reachable: 1, disabled });
		if (url === '/api/hue-lamps') return json({ success: true, lamps, total: 2, connected: 1, reachable: 1, message: '' });
		if (url === '/api/hue-lamps/scan') return scan;
		throw new Error(`unexpected ${url}`);
	});
}

describe('HueLamps', () => {
	it('lists the lamps with « 1 joignable sur 2 »', async () => {
		const calls = backend();
		await render(HueLamps);
		await expect.element(page.getByRole('region', { name: m.hue_lamps_title() })).toBeVisible();
		await expect.element(page.getByText(m.lamps_reachable_of({ count: 1, total: 2 }))).toBeVisible();
		await expect.element(page.getByRole('button', { name: 'Lampe du salon' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(m.lamps_never_seen())).toBeVisible();
		expect(calls.map((c) => c.url).sort()).toEqual(['/api/hue-lamps', '/api/hue-lamps/stats']);
	});

	it('is hidden when the server runs without Bluetooth', async () => {
		backend({ disabled: true });
		const { container } = await render(HueLamps);
		await expect.poll(() => container.querySelector('section')).toBeNull();
		await expect.element(page.getByText(m.hue_lamps_title())).not.toBeInTheDocument();
	});

	it('the scan button asks the server once, says so, then reads the lamps again', async () => {
		let answer!: (r: Response) => void;
		const calls = backend({ scan: new Promise((r) => (answer = r)) });
		await render(HueLamps);
		const scan = page.getByRole('button', { name: m.hue_lamps_scan() });
		await expect.element(page.getByRole('button', { name: 'Lampe du salon' })).toBeVisible();
		await scan.click();
		await expect.element(scan).toBeDisabled();
		const post = calls.find((c) => c.url === '/api/hue-lamps/scan');
		expect(post?.init?.method).toBe('POST');
		const before = calls.filter((c) => c.url === '/api/hue-lamps').length;
		answer(json({ success: true }));
		await expect.element(scan).toBeEnabled();
		expect(ui.toasts.map((t) => t.text)).toContain(m.hue_lamps_scan_started());
		expect(calls.filter((c) => c.url === '/api/hue-lamps').length).toBe(before + 1);
	});
});
