import { beforeEach, afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { json, stubFetch } from '#lib/test/fetch.ts';
import { zigbeeLamp } from '#lib/test/lamps.ts';
import ZigbeeLamps from './ZigbeeLamps.svelte';

afterEach(() => forgetAll());

function backend({ disabled = false, lamps = [zigbeeLamp()] } = {}) {
	return stubFetch((url) => {
		if (url === '/api/zigbee/lamps/stats') return json({ success: true, total: lamps.length, connected: 1, reachable: 1, disabled });
		if (url === '/api/zigbee/lamps') return json({ success: true, lamps, total: lamps.length, connected: lamps.length, reachable: 1, message: '' });
		if (url === '/api/zigbee/lamps/pairing/status')
			return json({ success: true, pairing: { active: false, remainingSeconds: 0, permitJoinSeconds: 0 }, message: '' });
		throw new Error(`unexpected ${url}`);
	});
}

describe('ZigbeeLamps', () => {
	beforeEach(() => session.adopt(leonard));

	it('lists the lamps of the coordinator', async () => {
		backend();
		await render(ZigbeeLamps);
		await expect.element(page.getByRole('region', { name: m.zigbee_lamps_title() })).toBeVisible();
		await expect.element(page.getByText(m.lamps_reachable_of({ count: 1, total: 1 }))).toBeVisible();
		await expect.element(page.getByRole('link', { name: 'Suspension' })).toHaveAttribute('href', '/zigbee-lamp/zb-1');
	});

	it('says why there is none', async () => {
		backend({ lamps: [] });
		await render(ZigbeeLamps);
		await expect.element(page.getByText(m.zigbee_lamps_no_lamps())).toBeVisible();
		await expect.element(page.getByText(m.zigbee_lamps_no_lamps_hint())).toBeVisible();
	});

	it('a member cannot pair new lamps', async () => {
		session.adopt(alex);
		backend();
		await render(ZigbeeLamps);
		await expect.element(page.getByRole('heading', { name: m.zigbee_lamps_title() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.action_pair() })).not.toBeInTheDocument();
	});

	it('« Appairer » unfolds the pairing panel, and folds it again', async () => {
		const calls = backend();
		await render(ZigbeeLamps);
		const pair = page.getByRole('button', { name: m.action_pair() });
		await expect.element(pair).toHaveAttribute('aria-expanded', 'false');
		// the pairing window is only read while the panel is open
		expect(calls.some((c) => c.url.includes('pairing'))).toBe(false);
		await pair.click();
		await expect.element(pair).toHaveAttribute('aria-expanded', 'true');
		await expect.element(page.getByText(m.zigbee_lamps_pairing_inactive())).toBeVisible();
		await pair.click();
		await expect.element(page.getByText(m.zigbee_lamps_pairing_inactive())).not.toBeInTheDocument();
	});

	it('is hidden when Zigbee is disabled', async () => {
		backend({ disabled: true });
		const { container } = await render(ZigbeeLamps);
		await expect.poll(() => container.querySelector('section')).toBeNull();
		await expect.element(page.getByText(m.zigbee_lamps_title())).not.toBeInTheDocument();
	});
});
