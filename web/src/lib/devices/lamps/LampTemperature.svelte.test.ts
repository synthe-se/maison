import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { hueLamp, zigbeeLamp } from '#lib/test/lamps.ts';
import LampTemperature from './LampTemperature.svelte';
import { fromHue, fromZigbee, hue, zigbee } from './lamp.ts';

afterEach(() => forgetAll());

describe('LampTemperature', () => {
	it('a Zigbee lamp’s white is said in kelvins', async () => {
		stubFetch(() => json({ success: true }));
		await render(LampTemperature, { lamp: { ...fromZigbee(zigbeeLamp()), temperature: 0 }, driver: zigbee });
		const slider = page.getByRole('slider', { name: m.lamps_temperature() });
		await expect.element(slider).toHaveAttribute('aria-valuetext', zigbee.temperatureText(0));
		await expect.element(slider).toBeEnabled();
	});

	it('PageDown warms it by 10 and sends the value', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await render(LampTemperature, { lamp: { ...fromHue(hueLamp()), temperature: 50 }, driver: hue });
		(page.getByRole('slider').element() as HTMLElement).focus();
		await userEvent.keyboard('{PageDown}');
		await expect.poll(() => calls.map((c) => [c.init?.method, c.url, sentBody(c)])).toEqual([['POST', '/api/hue-lamps/hue-1/temperature', { temperature: 40 }]]);
		await expect.element(page.getByRole('slider')).toHaveAttribute('aria-valuetext', hue.temperatureText(40));
	});

	it('an off lamp’s white cannot be changed', async () => {
		stubFetch(() => json({ success: true }));
		await render(LampTemperature, { lamp: { ...fromHue(hueLamp({ state: { isOn: false } })), temperature: 50 }, driver: hue });
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeDisabled();
	});
});
