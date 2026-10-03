import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import PlugTile from './PlugTile.svelte';

afterEach(() => {
	vi.useRealTimers();
	forgetAll();
});

describe('PlugTile', () => {
	it('names its toggle after the plug, pressed when on, links to its page, shows the power', async () => {
		stubFetch(() => json({ success: true }));
		await render(PlugTile, { id: 'p1', name: 'Radiateur', on: true, online: true, fact: '42 W' });
		await expect.element(page.getByRole('button', { name: 'Radiateur' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'Radiateur' })).toHaveAttribute('href', '/meross/p1');
		await expect.element(page.getByText(m.state_on())).toBeVisible();
		await expect.element(page.getByText('42 W')).toBeVisible();
	});

	it('the icon turns it on: one POST to toggle with the target', async () => {
		const calls = stubFetch(() => json({ success: true, on: true }));
		await render(PlugTile, { id: 'p1', name: 'Radiateur', on: false, online: true });
		await expect.element(page.getByText(m.state_off())).toBeVisible();
		await page.getByRole('button', { name: 'Radiateur' }).click();
		await expect.poll(() => calls.length).toBe(1);
		expect(calls[0].url).toBe('/api/meross/p1/toggle');
		expect(calls[0].init?.method).toBe('POST');
		expect(sentBody(calls[0])).toEqual({ on: true });
	});

	it('no answer within 5 s: « no answer » and a retry', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		const calls = stubFetch(() => new Promise<Response>(() => {}));
		await render(PlugTile, { id: 'p1', name: 'Radiateur', on: false, online: true });
		await page.getByRole('button', { name: 'Radiateur' }).click();
		await vi.advanceTimersByTimeAsync(5000);
		await expect.element(page.getByText(m.command_no_answer_short(), { exact: false })).toBeVisible();
		await page.getByRole('button', { name: m.common_retry() }).click();
		expect(calls).toHaveLength(2);
	});

	it('offline: its toggle unavailable, « unreachable since »; on its page, no link', async () => {
		stubFetch(() => json({}));
		await render(PlugTile, { id: 'p1', name: 'Radiateur', on: true, online: false, lastPing: Date.now() - 3 * 60_000, link: false });
		const since = m.state_unreachable_for({ duration: m.duration_minutes({ m: 3 }) });
		await expect.element(page.getByText(since)).toBeVisible();
		await expect.element(page.getByRole('button', { name: 'Radiateur' })).toHaveAttribute('aria-disabled', 'true');
		await expect.element(page.getByRole('link')).not.toBeInTheDocument();
	});
});
