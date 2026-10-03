import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { hueLamp, zigbeeLamp } from '#lib/test/lamps.ts';
import LampTile from './LampTile.svelte';
import { fromHue, fromZigbee, hue, zigbee } from './lamp.ts';

afterEach(() => {
	vi.useRealTimers();
});

describe('LampTile', () => {
	it('names its toggle after the lamp, pressed when on, and says the state in words', async () => {
		stubFetch(() => json({ success: true }));
		await render(LampTile, { lamp: fromHue(hueLamp()), driver: hue });
		const toggle = page.getByRole('button', { name: 'Lampe du salon' });
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'Lampe du salon' })).toHaveAttribute('href', '/hue-lamp/hue-1');
		await expect.element(page.getByText(m.lamps_on_percent({ percent: 80 }))).toBeVisible();
		const slider = page.getByRole('slider', { name: m.lamps_brightness() });
		await expect.element(slider).toHaveAttribute('aria-valuetext', expect.stringMatching(/^80\s?%$/));
		await expect.element(slider).toBeEnabled();
	});

	it('the icon turns it off: one POST to its power endpoint', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await render(LampTile, { lamp: fromHue(hueLamp()), driver: hue });
		await page.getByRole('button', { name: 'Lampe du salon' }).click();
		await expect.poll(() => calls.length).toBe(1);
		expect(calls[0].url).toBe('/api/hue-lamps/hue-1/power');
		expect(calls[0].init?.method).toBe('POST');
		expect(sentBody(calls[0])).toEqual({ enabled: false });
	});

	it('shows the target at once while the order travels', async () => {
		stubFetch(() => new Promise<Response>(() => {}));
		await render(LampTile, { lamp: fromZigbee(zigbeeLamp({ state: { isOn: false } })), driver: zigbee });
		const toggle = page.getByRole('button', { name: 'Suspension' });
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
		await toggle.click();
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(m.state_off())).toBeVisible();
	});

	it('no answer within 3 s: back to the read state, « Pas de réponse · Réessayer », said once', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		const say = vi.spyOn(ui, 'say');
		const calls = stubFetch(() => new Promise<Response>(() => {}));
		await render(LampTile, { lamp: fromHue(hueLamp()), driver: hue });
		await page.getByRole('button', { name: 'Lampe du salon' }).click();
		await vi.advanceTimersByTimeAsync(1000);
		await expect.element(page.getByText(m.command_turning_off())).toBeVisible();
		await vi.advanceTimersByTimeAsync(2000);
		await expect.element(page.getByText(m.command_no_answer_short(), { exact: false })).toBeVisible();
		expect(say).toHaveBeenCalledWith(m.command_no_answer({ name: 'Lampe du salon' }));
		await expect.element(page.getByRole('button', { name: 'Lampe du salon' })).toHaveAttribute('aria-pressed', 'true');
		await page.getByRole('button', { name: m.common_retry() }).click();
		expect(calls).toHaveLength(2);
		expect(sentBody(calls[1])).toEqual({ enabled: false });
	});

	it('PageUp moves the brightness by 10 and sends it', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await render(LampTile, { lamp: fromZigbee(zigbeeLamp()), driver: zigbee });
		const slider = page.getByRole('slider', { name: m.lamps_brightness() });
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{PageUp}');
		await expect.poll(() => calls.map((c) => [c.url, sentBody(c)])).toEqual([['/api/zigbee/lamps/zb-1/brightness', { brightness: 70 }]]);
	});

	it('unreachable: the toggle kept but unavailable, no slider, the state in the warning words', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await render(LampTile, { lamp: fromZigbee(zigbeeLamp({ reachable: false, lastSeen: null })), driver: zigbee });
		const toggle = page.getByRole('button', { name: 'Suspension' });
		await expect.element(toggle).toHaveAttribute('aria-disabled', 'true');
		await expect.element(toggle).toHaveAccessibleDescription(m.state_unreachable());
		(toggle.element() as HTMLElement).click();
		expect(calls).toEqual([]);
		await expect.element(page.getByText(m.state_unreachable())).toBeVisible();
		await expect.element(page.getByRole('slider')).not.toBeInTheDocument();
	});

	it('an off lamp is one compact row: no slider that looks draggable', async () => {
		stubFetch(() => json({ success: true }));
		await render(LampTile, { lamp: fromHue(hueLamp({ state: { isOn: false } })), driver: hue });
		await expect.element(page.getByText(m.state_off())).toBeVisible();
		await expect.element(page.getByRole('slider')).not.toBeInTheDocument();
	});

	it('on its own page the name is not a link', async () => {
		stubFetch(() => json({ success: true }));
		await render(LampTile, { lamp: fromHue(hueLamp()), driver: hue, link: false });
		await expect.element(page.getByRole('article', { name: 'Lampe du salon' })).toBeVisible();
		await expect.element(page.getByRole('link')).not.toBeInTheDocument();
	});
});
