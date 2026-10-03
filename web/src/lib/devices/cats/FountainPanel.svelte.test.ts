import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { formatDuration, formatMinutes } from '#lib/format.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { fact, facts } from '#lib/test/facts.ts';
import FountainPanel from './FountainPanel.svelte';

const STATUS = '/devices/w1/fountain/status';
const fountain = (parsedStatus: unknown, more: Record<string, unknown> = {}) =>
	stubApi({ [STATUS]: { success: true, parsedStatus }, ...more });

describe('FountainPanel', () => {
	it('says it is loading until the fountain answers', async () => {
		stubApi({ [STATUS]: () => new Promise(() => {}) });
		await render(FountainPanel, { id: 'w1' });
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
	});

	it('shows its settings as switches, its levels and counters in words', async () => {
		fountain({ power: true, uvRuntime: 600, ecoMode: 2, waterLevel: 'low', filterLife: 90, pumpTime: 30, waterTime: 1440 });
		await render(FountainPanel, { id: 'w1' });
		await expect.element(page.getByRole('switch', { name: m.fountain_power() })).toBeChecked();
		await expect.element(page.getByRole('switch', { name: m.fountain_uv_sterilization() })).toBeChecked();
		await expect.element(page.getByText(m.fountain_uv_runtime({ time: formatDuration(600) }))).toBeVisible();
		await expect.element(page.getByRole('radio', { name: m.fountain_mode2() })).toHaveAttribute('aria-checked', 'true');
		await expect.element(page.getByRole('radio', { name: m.fountain_mode1() })).toHaveAttribute('aria-checked', 'false');
		expect(facts()).toEqual([
			{ term: m.fountain_water_level(), value: m.device_level_low(), warn: true },
			{ term: m.fountain_fresh_water(), value: formatMinutes(1440), warn: false },
			{ term: m.fountain_filter_time(), value: formatMinutes(90), warn: false },
			{ term: m.fountain_pump_time(), value: formatMinutes(30), warn: false }
		]);
		await expect.element(page.getByText(m.fountain_add_water())).toBeVisible();
	});

	it('takes the UV state from its switch, and says an unknown water level', async () => {
		fountain({ power: false, uv: true });
		await render(FountainPanel, { id: 'w1' });
		await expect.element(page.getByRole('switch', { name: m.fountain_uv_sterilization() })).toBeChecked();
		await expect.element(page.getByRole('switch', { name: m.fountain_power() })).not.toBeChecked();
		expect(facts()).toEqual([{ term: m.fountain_water_level(), value: m.common_unknown(), warn: false }]);
	});

	it('names a level it knows, or shows the device’s own word', async () => {
		fountain({ waterLevel: 'ok' });
		await render(FountainPanel, { id: 'w1' });
		await expect.element(page.getByText(m.fountain_water_level_ok())).toBeVisible();
		expect(fact(m.fountain_water_level())?.warn).toBe(false);
	});

	it('switches the fountain off, reads it again, and says so', async () => {
		const api = fountain({ power: true }, { 'POST /devices/w1/fountain/power': { success: true } });
		const say = vi.spyOn(ui, 'say');
		await render(FountainPanel, { id: 'w1' });
		await expect.element(page.getByRole('switch', { name: m.fountain_power() })).toBeChecked();
		api.routes[STATUS] = { success: true, parsedStatus: { power: false } };
		await page.getByRole('switch', { name: m.fountain_power() }).click();
		await expect.element(page.getByRole('switch', { name: m.fountain_power() })).not.toBeChecked();
		expect(api.sent('POST', '/devices/w1/fountain/power')[0].body).toEqual({ enabled: false });
		await expect.poll(() => say).toHaveBeenCalledWith(m.fountain_fountain_off());
	});

	it('switches the UV on, and chooses an eco mode', async () => {
		const api = fountain(
			{},
			{ 'POST /devices/w1/fountain/uv': { success: true }, 'POST /devices/w1/fountain/eco-mode': { success: true } }
		);
		const say = vi.spyOn(ui, 'say');
		await render(FountainPanel, { id: 'w1' });
		await page.getByRole('switch', { name: m.fountain_uv_sterilization() }).click();
		await expect.poll(() => api.sent('POST', '/devices/w1/fountain/uv')[0]?.body).toEqual({ enabled: true });
		await expect.poll(() => say).toHaveBeenCalledWith(m.fountain_uv_on());
		await page.getByRole('radio', { name: m.fountain_mode1() }).click();
		await expect.poll(() => api.sent('POST', '/devices/w1/fountain/eco-mode')[0]?.body).toEqual({ mode: 1 });
		await expect.poll(() => say).toHaveBeenCalledWith(m.fountain_eco_mode_on({ mode: 1 }));
	});

	it('resets a counter only once confirmed', async () => {
		const api = fountain({ filterLife: 90 }, { 'POST /devices/w1/fountain/reset/filter': { success: true } });
		await render(FountainPanel, { id: 'w1' });
		await page.getByRole('button', { name: m.fountain_reset_filter_changed() }).click();
		const dialog = page.getByRole('alertdialog', { name: m.fountain_reset_filter_title() });
		await expect.element(dialog).toHaveAccessibleDescription(m.fountain_reset_description());
		await dialog.getByRole('button', { name: m.device_keep() }).click();
		await expect.element(dialog).not.toBeInTheDocument();
		expect(api.sent('POST', '/devices/w1/fountain/reset/filter')).toHaveLength(0);
		await page.getByRole('button', { name: m.fountain_reset_filter_changed() }).click();
		await page.getByRole('button', { name: m.device_reset() }).click();
		await expect.poll(() => api.sent('POST', '/devices/w1/fountain/reset/filter')).toHaveLength(1);
	});

	it('resets the water and pump counters through their own endpoints', async () => {
		const api = fountain(
			{},
			{ 'POST /devices/w1/fountain/reset/water': { success: true }, 'POST /devices/w1/fountain/reset/pump': { success: true } }
		);
		const say = vi.spyOn(ui, 'say');
		await render(FountainPanel, { id: 'w1' });
		await page.getByRole('button', { name: m.fountain_water_changed() }).click();
		await page.getByRole('button', { name: m.device_reset() }).click();
		await expect.poll(() => say).toHaveBeenCalledWith(m.fountain_water_counter_reset());
		await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.fountain_reset_pump_cleaned() }).click();
		await page.getByRole('button', { name: m.device_reset() }).click();
		await expect.poll(() => say).toHaveBeenCalledWith(m.fountain_pump_counter_reset());
		expect(api.calls.filter((c) => c.method === 'POST').map((c) => c.path)).toEqual([
			'/devices/w1/fountain/reset/water',
			'/devices/w1/fountain/reset/pump'
		]);
	});

	it('closes the confirmation once confirmed', async () => {
		fountain({}, { 'POST /devices/w1/fountain/reset/water': { success: true } });
		await render(FountainPanel, { id: 'w1' });
		await page.getByRole('button', { name: m.fountain_water_changed() }).click();
		await page.getByRole('button', { name: m.device_reset() }).click();
		await expect.element(page.getByRole('alertdialog'), { timeout: 1000 }).not.toBeInTheDocument();
	});
});
