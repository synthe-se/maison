import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import { hueLamp, zigbeeLamp } from '#lib/test/lamps.ts';
import LampsGroup from './LampsGroup.svelte';

afterEach(() => {
	ui.toasts = [];
});

const stats = (disabled = false) => ({ success: true, total: 1, connected: 1, reachable: 1, disabled });
const hueList = (
	lamps = [hueLamp(), hueLamp({ id: 'hue-2', name: 'Chevet', connected: false, lastSeen: null, state: { isOn: false } })]
) => ({
	success: true,
	lamps,
	total: lamps.length,
	connected: 1,
	reachable: 1,
	message: ''
});
const zbList = (lamps = [zigbeeLamp()]) => ({
	success: true,
	lamps,
	total: lamps.length,
	connected: lamps.length,
	reachable: 1,
	message: ''
});

/** The backend: both radios, every command answered. */
function backend(over: Record<string, unknown> = {}) {
	return stubApi({
		'/hue-lamps/stats': stats(),
		'/zigbee/lamps/stats': stats(),
		'/hue-lamps': hueList(),
		'/zigbee/lamps': zbList(),
		'/zigbee/lamps/pairing/status': { success: true, pairing: { active: false, remainingSeconds: 0, permitJoinSeconds: 0 }, message: '' },
		'POST /hue-lamps/hue-1/power': { success: true },
		'POST /zigbee/lamps/zb-1/power': { success: true },
		'POST /hue-lamps/scan': { success: true },
		...over
	});
}

describe('LampsGroup', () => {
	beforeEach(() => session.adopt(leonard));

	it('one « Lampes » group for both radios, each lamp linking to its own page', async () => {
		backend();
		await render(LampsGroup);
		const group = page.getByRole('region', { name: m.zigbee_lamps_title() });
		await expect.element(group).toBeVisible();
		await expect.element(group.getByRole('link', { name: 'Lampe du salon' })).toHaveAttribute('href', '/hue-lamp/hue-1');
		await expect.element(group.getByRole('link', { name: 'Suspension' })).toHaveAttribute('href', '/zigbee-lamp/zb-1');
		await expect.element(page.getByText(m.state_never_seen())).toBeVisible();
	});

	it('leaves out a radio the server runs without, and hides when it has none', async () => {
		const api = backend({ '/hue-lamps/stats': stats(true) });
		const view = await render(LampsGroup);
		await expect.element(page.getByRole('link', { name: 'Suspension' })).toBeVisible();
		await expect.element(page.getByRole('link', { name: 'Lampe du salon' })).not.toBeInTheDocument();
		await view.unmount();
		api.routes['/zigbee/lamps/stats'] = stats(true);
		const { container } = await render(LampsGroup);
		await expect.poll(() => api.calls.filter((c) => c.path === '/zigbee/lamps/stats').length).toBeGreaterThan(1);
		await expect.poll(() => container.querySelector('section')).toBeNull();
	});

	it('a list that cannot be read and nothing known: says so, never « no lamp »', async () => {
		backend({
			'/hue-lamps': new Response('{"error":"Bluetooth down"}', { status: 500 }),
			'/zigbee/lamps': new Response('{"error":"down"}', { status: 500 })
		});
		await render(LampsGroup);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByText(m.lamps_none())).not.toBeInTheDocument();
	});

	it('says why there is no lamp', async () => {
		backend({ '/hue-lamps': hueList([]), '/zigbee/lamps': zbList([]) });
		await render(LampsGroup);
		await expect.element(page.getByText(m.lamps_none())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.lamps_all_off() })).not.toBeInTheDocument();
	});

	it('« Tout éteindre » turns off every lit lamp at once and says how many', async () => {
		const api = backend();
		await render(LampsGroup);
		await page.getByRole('button', { name: m.lamps_all_off() }).click();
		await expect
			.poll(() => api.calls.filter((c) => c.method === 'POST').map((c) => [c.path, c.body]))
			.toEqual(
				expect.arrayContaining([
					['/hue-lamps/hue-1/power', { enabled: false }],
					['/zigbee/lamps/zb-1/power', { enabled: false }]
				])
			);
		// the off and unreachable lamp is not asked
		expect(api.calls.some((c) => c.path === '/hue-lamps/hue-2/power')).toBe(false);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.lamps_all_off_done({ count: 2 }));
	});

	it('once none is lit, « Tout éteindre » goes and the focus moves to the group’s title', async () => {
		const api = backend();
		await render(LampsGroup);
		await expect.element(page.getByRole('button', { name: m.lamps_all_off() })).toBeVisible();
		api.routes['/hue-lamps'] = hueList([hueLamp({ state: { isOn: false } })]);
		api.routes['/zigbee/lamps'] = zbList([zigbeeLamp({ state: { isOn: false } })]);
		await page.getByRole('button', { name: m.lamps_all_off() }).click();
		await expect.element(page.getByRole('button', { name: m.lamps_all_off() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('heading', { name: m.zigbee_lamps_title() })).toHaveFocus();
	});

	it('« Tout éteindre » names a lamp that did not answer, in a warning that stays', async () => {
		backend({ 'POST /zigbee/lamps/zb-1/power': new Response('{"error":"timeout"}', { status: 504 }) });
		await render(LampsGroup);
		const button = page.getByRole('button', { name: m.lamps_all_off() });
		await button.click();
		await expect
			.poll(() => ui.toasts.find((t) => t.warn)?.text)
			.toBe(m.group_partial({ done: m.lamps_all_off_done({ count: 1 }), names: 'Suspension' }));
	});

	it('« Tout éteindre » keeps the focus and says it is busy while it travels', async () => {
		const wait = deferred();
		backend({ 'POST /hue-lamps/hue-1/power': () => wait.promise.then(() => ({ success: true })) });
		await render(LampsGroup);
		const button = page.getByRole('button', { name: m.lamps_all_off() });
		await button.click();
		await expect.element(button).toHaveAttribute('aria-busy', 'true');
		await expect.element(button).toHaveFocus();
		wait.resolve(undefined);
		await expect.element(button).not.toHaveAttribute('aria-busy');
	});

	it('an admin adds a lamp by Bluetooth (a search) or Zigbee (opening the network)', async () => {
		const api = backend();
		await render(LampsGroup);
		await page.getByRole('button', { name: m.lamps_add() }).click();
		await page.getByRole('menuitem', { name: m.lamps_add_bluetooth() }).click();
		await expect.poll(() => api.sent('POST', '/hue-lamps/scan')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.hue_lamps_scan_started());
		await page.getByRole('button', { name: m.lamps_add() }).click();
		await page.getByRole('menuitem', { name: m.lamps_add_zigbee() }).click();
		await expect.element(page.getByText(m.zigbee_lamps_pairing_inactive())).toBeVisible();
	});

	it('a member sees the lamps, not the way to add one', async () => {
		session.adopt(alex);
		backend();
		await render(LampsGroup);
		await expect.element(page.getByRole('button', { name: 'Lampe du salon' })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.lamps_add() })).not.toBeInTheDocument();
	});
});
