import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { goto } from '$app/navigation';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { ui } from '#lib/ui.svelte.ts';
import { json, stubFetch } from '#lib/test/fetch.ts';
import { hueLamp } from '#lib/test/lamps.ts';
import HueLampPage from './HueLampPage.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));

afterEach(() => {
	ui.toasts = [];
});

function backend(lamp = hueLamp()) {
	return stubFetch((url) => {
		if (url === '/api/hue-lamps/hue-1') return json({ success: true, lamp });
		if (url === '/api/hue-lamps/hue-1/blacklist') return json({ success: true });
		if (url === '/api/hue-lamps/missing') return json({ success: false, error: 'not found' });
		throw new Error(url);
	});
}

describe('HueLampPage', () => {
	beforeEach(() => session.adopt(leonard));

	it('reads its lamp and shows it with its Bluetooth address', async () => {
		const calls = backend();
		await render(HueLampPage, { id: 'hue-1' });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Lampe du salon');
		await expect.element(page.getByText(m.hue_lamps_address())).toBeVisible();
		await expect.element(page.getByText('AA:BB:CC:DD:EE:FF')).toBeVisible();
		expect(calls.map((c) => c.url)).toEqual(['/api/hue-lamps/hue-1']);
		await expect.element(page.getByText(m.hue_lamps_not_connected_description())).not.toBeInTheDocument();
	});

	it('not connected: says why it may be', async () => {
		backend(hueLamp({ connected: false }));
		await render(HueLampPage, { id: 'hue-1' });
		await expect.element(page.getByText(m.hue_lamps_not_connected_description())).toBeVisible();
	});

	it('an unknown lamp: « Lampe introuvable »', async () => {
		backend();
		await render(HueLampPage, { id: 'missing' });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent(m.lamps_not_found());
	});

	it('a member cannot hide a lamp', async () => {
		session.adopt(alex);
		backend();
		await render(HueLampPage, { id: 'hue-1' });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Lampe du salon');
		await expect.element(page.getByRole('button', { name: m.hue_lamps_blacklist() })).not.toBeInTheDocument();
	});

	it('hiding asks first, then POSTs, says so and goes home', async () => {
		const calls = backend();
		await render(HueLampPage, { id: 'hue-1' });
		await page.getByRole('button', { name: m.hue_lamps_blacklist() }).click();
		const dialog = page.getByRole('alertdialog');
		await expect.element(dialog).toHaveAccessibleName(m.hue_lamps_blacklist_title({ name: 'Lampe du salon' }));
		expect(calls.some((c) => c.url.endsWith('/blacklist'))).toBe(false);
		await dialog.getByRole('button', { name: m.hue_lamps_blacklist() }).click();
		await expect.poll(() => vi.mocked(goto).mock.calls).toContainEqual(['/']);
		const post = calls.filter((c) => c.url === '/api/hue-lamps/hue-1/blacklist');
		expect(post).toHaveLength(1);
		expect(post[0].init?.method).toBe('POST');
		expect(ui.toasts.map((t) => t.text)).toContain(m.hue_lamps_blacklisted({ name: 'Lampe du salon' }));
	});
});
