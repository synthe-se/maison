import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { zigbeeLamp } from '#lib/test/lamps.ts';
import type { ZigbeeLamp } from '#lib/api.ts';
import ZigbeeLampDetail from './ZigbeeLampDetail.svelte';

afterEach(() => {
	vi.useRealTimers();
	forgetAll();
});

/** The lamp's endpoint and every order; `lamp` is what a read answers, `hold` delays a read. */
function backend(lamp: ZigbeeLamp) {
	const state: { lamp: ZigbeeLamp; hold?: Promise<void> } = { lamp };
	const calls = stubFetch(async (url, init) => {
		if (url === '/api/zigbee/lamps/zb-1' && (init?.method ?? 'GET') === 'GET') {
			const answer = json({ success: true, lamp: state.lamp });
			await state.hold;
			return answer;
		}
		if (url.endsWith('/rename')) state.lamp = { ...state.lamp, name: sentBody({ url, init }).name };
		return json({ success: true, message: '' });
	});
	return { calls, state, orders: () => calls.filter((c) => c.init?.method === 'POST').map((c) => [c.url, sentBody(c)]) };
}

describe('ZigbeeLampDetail', () => {
	it('shows its network details', async () => {
		backend(zigbeeLamp({ linkQuality: null, interviewCompleted: false }));
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Suspension');
		await expect.element(page.getByText('0x0017880104b2c3d4')).toBeVisible();
		await expect.element(page.getByText('00:17:88:01:0b:2c:3d:4e')).toBeVisible();
		await expect.element(page.getByText(m.zigbee_lamps_interview_pending())).toBeVisible();
		const quality = [...document.querySelectorAll('dl div')].find((d) => d.querySelector('dt')?.textContent === m.zigbee_lamps_link_quality());
		expect(quality?.querySelector('dd')?.textContent).toBe(m.common_unknown());
	});

	it('a colour lamp in white mode opens on the white tab', async () => {
		backend(zigbeeLamp({ state: { colorMode: 2 } }));
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		await expect.element(page.getByRole('tab', { name: m.color_white() })).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeVisible();
	});

	it('in colour mode opens on the colour tab; back to white sends the last temperature', async () => {
		const { orders } = backend(zigbeeLamp({ state: { colorMode: 0, temperature: 30 } }));
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		const colour = page.getByRole('tab', { name: m.zigbee_lamps_color() });
		await expect.element(colour).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByRole('group', { name: m.zigbee_lamps_color() })).toBeInTheDocument();
		await page.getByRole('tab', { name: m.color_white() }).click();
		await expect.poll(orders).toEqual([['/api/zigbee/lamps/zb-1/temperature', { temperature: 30 }]]);
		// going to colour sends nothing: the person picks one
		await colour.click();
		await expect.element(colour).toHaveAttribute('aria-selected', 'true');
		expect(orders()).toHaveLength(1);
	});

	it('the chosen tab stays when a poll says another mode', async () => {
		const { state } = backend(zigbeeLamp({ state: { colorMode: 2 } }));
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		await page.getByRole('tab', { name: m.zigbee_lamps_color() }).click();
		state.lamp = zigbeeLamp({ state: { colorMode: 2, brightness: 61 } });
		await new Promise((r) => setTimeout(r, 3300));
		await expect.element(page.getByText(m.lamps_on_percent({ percent: 61 }))).toBeVisible();
		await expect.element(page.getByRole('tab', { name: m.zigbee_lamps_color() })).toHaveAttribute('aria-selected', 'true');
	});

	it('a colour lamp with no white tuning shows the colours alone', async () => {
		backend(zigbeeLamp({ state: { temperature: null, colorMode: 0 } }));
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		await expect.element(page.getByRole('group', { name: m.zigbee_lamps_color() })).toBeInTheDocument();
		await expect.element(page.getByRole('tab')).not.toBeInTheDocument();
	});

	it('a white-only lamp gets the plain temperature slider', async () => {
		backend(zigbeeLamp({ supportsColor: false }));
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeVisible();
		await expect.element(page.getByRole('tab')).not.toBeInTheDocument();
	});

	it('renaming: the button waits for a new name, sends it trimmed and says it', async () => {
		const say = vi.spyOn(ui, 'say');
		const { calls } = backend(zigbeeLamp());
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		const input = page.getByLabelText(m.zigbee_lamps_name());
		const button = page.getByRole('button', { name: m.zigbee_lamps_rename_confirm() });
		await expect.element(input).toHaveValue('Suspension');
		await expect.element(button).toBeDisabled();
		await input.fill('  Plafonnier ');
		await expect.element(button).toBeEnabled();
		await userEvent.keyboard('{Enter}');
		await expect.poll(() => say.mock.calls.flat()).toContain(m.zigbee_lamps_renamed({ name: 'Plafonnier' }));
		const rename = calls.filter((c) => c.url === '/api/zigbee/lamps/zb-1/rename');
		expect(rename.map((c) => [c.init?.method, sentBody(c)])).toEqual([['POST', { name: 'Plafonnier' }]]);
	});

	it('a poll in flight during a rename does not make it say the old name', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		const say = vi.spyOn(ui, 'say');
		const b = backend(zigbeeLamp());
		await render(ZigbeeLampDetail, { id: 'zb-1' });
		const input = page.getByLabelText(m.zigbee_lamps_name());
		await expect.element(input).toHaveValue('Suspension');
		// the next poll starts, and is slow
		let release!: () => void;
		b.state.hold = new Promise((r) => (release = r));
		await vi.advanceTimersByTimeAsync(3000);
		await expect.poll(() => b.calls.length).toBe(2);
		await input.fill('Plafonnier');
		await userEvent.keyboard('{Enter}');
		await expect.poll(() => b.calls.some((c) => c.url.endsWith('/rename'))).toBe(true);
		release();
		await expect.poll(() => say.mock.calls.length).toBe(1);
		vi.useRealTimers();
		expect(say).toHaveBeenCalledWith(m.zigbee_lamps_renamed({ name: 'Plafonnier' }));
	});
});
