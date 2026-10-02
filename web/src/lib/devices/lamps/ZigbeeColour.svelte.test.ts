import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { zigbeeLamp } from '#lib/test/lamps.ts';
// the app's sizes: the swatches are empty buttons sized by its tokens
import '../../../styles/app.css';
import ZigbeeColour from './ZigbeeColour.svelte';
import { EFFECTS, PRESETS, rgbToXy } from './color.ts';

afterEach(() => forgetAll());

describe('ZigbeeColour', () => {
	it('named swatches in a group: each is a button a reader can name', async () => {
		stubFetch(() => json({ success: true }));
		await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		await expect.element(page.getByRole('group', { name: m.zigbee_lamps_color() })).toBeVisible();
		for (const p of PRESETS) await expect.element(page.getByRole('button', { name: p.name(), exact: true })).toBeEnabled();
	});

	it('a swatch sends its colour in xy', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		await page.getByRole('button', { name: PRESETS[0].name(), exact: true }).click();
		await expect.poll(() => calls.length).toBe(1);
		expect(calls[0].url).toBe('/api/zigbee/lamps/zb-1/color');
		expect(calls[0].init?.method).toBe('POST');
		expect(sentBody(calls[0])).toEqual(rgbToXy(PRESETS[0].rgb));
	});

	it('an effect, and stopping it', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		await page.getByRole('button', { name: EFFECTS[0].name() }).click();
		await expect.element(page.getByRole('button', { name: m.zigbee_lamps_effect_stop_effect() })).toBeEnabled();
		await page.getByRole('button', { name: m.zigbee_lamps_effect_stop_effect() }).click();
		await expect.poll(() => calls.map((c) => [c.url, sentBody(c)])).toEqual([
			['/api/zigbee/lamps/zb-1/effect', { effect: 'candle' }],
			['/api/zigbee/lamps/zb-1/effect', { effect: 'stop_hue_effect' }]
		]);
	});

	it('one gesture at a time: everything waits while a colour travels', async () => {
		let answer!: (r: Response) => void;
		stubFetch(() => new Promise<Response>((r) => (answer = r)));
		await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		await page.getByRole('button', { name: PRESETS[1].name(), exact: true }).click();
		await expect.element(page.getByRole('button', { name: EFFECTS[0].name() })).toBeDisabled();
		answer(json({ success: true }));
		await expect.element(page.getByRole('button', { name: EFFECTS[0].name() })).toBeEnabled();
	});

	it('the wheel picks the colour under the pointer; it is hidden from readers', async () => {
		const calls = stubFetch(() => json({ success: true }));
		const { container } = await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		const canvas = container.querySelector('canvas')!;
		expect(canvas.closest('[aria-hidden="true"]')).not.toBeNull();
		const box = canvas.getBoundingClientRect();
		// the right edge of the wheel, mid-height: pure red
		canvas.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: box.left + box.width - 1, clientY: box.top + box.height / 2 }));
		await expect.poll(() => calls.length).toBe(1);
		const { x, y } = sentBody(calls[0]);
		const red = rgbToXy([255, 0, 0]);
		expect(Math.abs(x - red.x) < 0.02 && Math.abs(y - red.y) < 0.02).toBe(true);
		// outside the disc (a corner): nothing is sent
		canvas.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: box.left + 1, clientY: box.top + 1 }));
		await new Promise((r) => setTimeout(r, 50));
		expect(calls).toHaveLength(1);
	});

	it('marks the lamp’s colour on the wheel, or nothing when unknown', async () => {
		stubFetch(() => json({ success: true }));
		const { container } = await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		await expect.poll(() => container.querySelector('.marker')).not.toBeNull();
		const none = await render(ZigbeeColour, { lamp: zigbeeLamp({ state: { colorX: null, colorY: null } }), disabled: false });
		expect(none.container.querySelector('.marker')).toBeNull();
	});

	it('off or unreachable: nothing can be sent', async () => {
		const calls = stubFetch(() => json({ success: true }));
		const { container } = await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: true });
		for (const p of PRESETS) await expect.element(page.getByRole('button', { name: p.name(), exact: true })).toBeDisabled();
		await expect.element(page.getByRole('button', { name: m.zigbee_lamps_effect_stop_effect() })).toBeDisabled();
		container.querySelector('canvas')!.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: 0, clientY: 0 }));
		expect(calls).toHaveLength(0);
	});

	it('a failure is told', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		stubFetch(() => json({ success: false, error: 'Lamp unreachable' }, 503));
		await render(ZigbeeColour, { lamp: zigbeeLamp(), disabled: false });
		await page.getByRole('button', { name: PRESETS[0].name(), exact: true }).click();
		await expect.poll(() => fail.mock.calls.length).toBe(1);
		expect(String(fail.mock.calls[0][0])).toContain('Lamp unreachable');
	});
});
