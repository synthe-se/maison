import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { zigbeeLamp } from '#lib/test/lamps.ts';
import type { ZigbeeLamp } from '#lib/devices/lamps/api.ts';
import ZigbeeLampPage from './ZigbeeLampPage.svelte';

afterEach(() => {
	vi.useRealTimers();
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

describe('ZigbeeLampPage', () => {
	it('shows its network details', async () => {
		backend(zigbeeLamp({ interviewCompleted: false }));
		await render(ZigbeeLampPage, { id: 'zb-1' });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Suspension');
		await expect.element(page.getByText('0x0017880104b2c3d4')).toBeVisible();
		await expect.element(page.getByText('00:17:88:01:0b:2c:3d:4e')).toBeVisible();
		await expect.element(page.getByText(m.zigbee_lamps_interview_pending())).toBeVisible();
		// the coordinator reads neither the link quality nor the firmware: not shown
		const terms = [...document.querySelectorAll('dl dt')].map((d) => d.textContent);
		expect(terms).not.toContain(m.device_firmware());
	});

	it('a color lamp in white mode opens on the white tab', async () => {
		backend(zigbeeLamp({ state: { colorMode: 2 } }));
		await render(ZigbeeLampPage, { id: 'zb-1' });
		await expect.element(page.getByRole('tab', { name: m.color_white() })).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeVisible();
	});

	it('in color mode opens on the color tab; back to white sends the last temperature', async () => {
		const { orders } = backend(zigbeeLamp({ state: { colorMode: 0, temperature: 30 } }));
		await render(ZigbeeLampPage, { id: 'zb-1' });
		const color = page.getByRole('tab', { name: m.zigbee_lamps_color() });
		await expect.element(color).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByRole('group', { name: m.zigbee_lamps_color() })).toBeInTheDocument();
		await page.getByRole('tab', { name: m.color_white() }).click();
		await expect.poll(orders).toEqual([['/api/zigbee/lamps/zb-1/temperature', { temperature: 30 }]]);
		// going to color sends nothing: the person picks one
		await color.click();
		await expect.element(color).toHaveAttribute('aria-selected', 'true');
		expect(orders()).toHaveLength(1);
	});

	it('the chosen tab stays when a poll says another mode', async () => {
		const { state } = backend(zigbeeLamp({ state: { colorMode: 2 } }));
		await render(ZigbeeLampPage, { id: 'zb-1' });
		await page.getByRole('tab', { name: m.zigbee_lamps_color() }).click();
		state.lamp = zigbeeLamp({ state: { colorMode: 2, brightness: 61 } });
		await new Promise((r) => setTimeout(r, 3300));
		await expect.element(page.getByText(m.lamps_on_percent({ percent: 61 }))).toBeVisible();
		await expect.element(page.getByRole('tab', { name: m.zigbee_lamps_color() })).toHaveAttribute('aria-selected', 'true');
	});

	it('a color lamp with no white tuning shows the colors alone', async () => {
		backend(zigbeeLamp({ state: { temperature: null, colorMode: 0 } }));
		await render(ZigbeeLampPage, { id: 'zb-1' });
		await expect.element(page.getByRole('group', { name: m.zigbee_lamps_color() })).toBeInTheDocument();
		await expect.element(page.getByRole('tab')).not.toBeInTheDocument();
	});

	it('a white-only lamp gets the plain temperature slider', async () => {
		backend(zigbeeLamp({ supportsColor: false }));
		await render(ZigbeeLampPage, { id: 'zb-1' });
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeVisible();
		await expect.element(page.getByRole('tab')).not.toBeInTheDocument();
	});

	it('renaming: sends the name trimmed, says it, the focus staying in the field', async () => {
		const say = vi.spyOn(ui, 'say');
		const { calls } = backend(zigbeeLamp());
		await render(ZigbeeLampPage, { id: 'zb-1' });
		const input = page.getByLabelText(m.zigbee_lamps_name());
		await expect.element(input).toHaveValue('Suspension');
		await input.fill('  Plafonnier ');
		await userEvent.keyboard('{Enter}');
		await expect.element(input).toHaveFocus();
		await expect.poll(() => say.mock.calls.flat()).toContain(m.common_renamed({ name: 'Plafonnier' }));
		const rename = calls.filter((c) => c.url === '/api/zigbee/lamps/zb-1/rename');
		expect(rename.map((c) => [c.init?.method, sentBody(c)])).toEqual([['POST', { name: 'Plafonnier' }]]);
	});

	it('a poll in flight during a rename does not make it say the old name', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		const say = vi.spyOn(ui, 'say');
		const b = backend(zigbeeLamp());
		await render(ZigbeeLampPage, { id: 'zb-1' });
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
		expect(say).toHaveBeenCalledWith(m.common_renamed({ name: 'Plafonnier' }));
	});
});
