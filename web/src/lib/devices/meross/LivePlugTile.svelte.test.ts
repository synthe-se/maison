import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { time } from '#lib/clock.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { TARIFFS, tempoToday } from '#lib/test/tempo.ts';
import { costPerHour, perHour } from '#lib/devices/tempo/price.ts';
import LivePlugTile from './LivePlugTile.svelte';
import { powerAndCost, watts } from './units.ts';
import { merossElectricity, merossPlug } from '#lib/test/meross.ts';

afterEach(() => {
	time.now = new Date();
});

/** A plug drawing `mw` milliwatts. */
const drawing = (mw: number) => {
	const e = merossElectricity();
	e.electricity.raw.power = mw;
	return e;
};

describe('LivePlugTile', () => {
	it('reads the plug’s power and says it with its cost at the price in force', async () => {
		time.now = new Date('2026-12-10T23:00:00'); // red day, off-peak
		const api = stubApi({ '/meross/p1/electricity': merossElectricity(), '/tempo': tempoToday() });
		await render(LivePlugTile, { plug: merossPlug() });
		await expect.element(page.getByText(powerAndCost(42.4, TARIFFS.red.offPeak))).toBeVisible();
		expect(api.calls.map((c) => c.path).toSorted()).toEqual(['/meross/p1/electricity', '/tempo']);
		await expect.element(page.getByRole('button', { name: 'Radiateur' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(m.state_on())).not.toHaveClass('warn');
	});

	it('without Tempo, the power alone', async () => {
		stubApi({ '/meross/p1/electricity': merossElectricity(), '/tempo': new Response('{}', { status: 503 }) });
		await render(LivePlugTile, { plug: merossPlug() });
		await expect.element(page.getByText(watts(42.4), { exact: true })).toBeVisible();
	});

	it('drawing power in a red day’s peak hours: a warning line with what it costs', async () => {
		time.now = new Date('2026-12-10T18:00:00');
		stubApi({ '/meross/p1/electricity': drawing(1_040_000), '/tempo': tempoToday() });
		await render(LivePlugTile, { plug: merossPlug() });
		const line = page.getByText(m.meross_red_peak({ cost: perHour(costPerHour(1040, TARIFFS.red.peak)) }));
		await expect.element(line).toBeVisible();
		await expect.element(line).toHaveClass('warn');
	});

	it('a plug off or idle in red peak hours is not flagged', async () => {
		time.now = new Date('2026-12-10T18:00:00');
		stubApi({ '/meross/p1/electricity': drawing(1_000), '/tempo': tempoToday() });
		await render(LivePlugTile, { plug: merossPlug() });
		await expect.element(page.getByText(m.state_on())).toBeVisible();
	});
});
