import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { percent } from '#lib/i18n.svelte.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import { fact, facts } from '#lib/test/facts.ts';
import FeederControl from './FeederControl.svelte';

const STATUS = '/devices/f1/feeder/status';
const MEALS = '/devices/f1/feeder/meal-plan';
const withStatus = (parsed_status: unknown, more: Record<string, unknown> = {}) =>
	stubApi({ [STATUS]: { success: true, parsed_status }, [MEALS]: { success: true, decoded: [] }, ...more });

describe('FeederControl', () => {
	afterEach(() => forgetAll());

	it('says what the feeder reports, in words, warning on what needs a hand', async () => {
		withStatus({ food_level: 'low', battery_level: 15, is_feeding: false, system: { powered_by: 'Battery', fault_status: true }, error: 'jam' });
		await render(FeederControl, { id: 'f1' });
		await expect.element(page.getByText(m.feeder_fault())).toBeVisible();
		expect(fact(m.feeder_food_level())).toEqual({ term: m.feeder_food_level(), value: m.device_level_low(), warn: true });
		expect(fact(m.feeder_battery())).toEqual({ term: m.feeder_battery(), value: percent(0.15), warn: true });
		expect(fact(m.feeder_power_source())?.value).toBe(m.feeder_power_battery());
		expect(facts().filter((f) => f.term === m.common_status()).map((f) => f.value)).toEqual([m.feeder_waiting(), m.feeder_fault()]);
		expect(fact(m.common_error())).toEqual({ term: m.common_error(), value: 'jam', warn: true });
	});

	it('says only what it knows, and the raw value of a level it does not name', async () => {
		withStatus({ food_level: 'overflowing', battery_level: 90, is_feeding: true, system: { powered_by: 'AC Power' } });
		await render(FeederControl, { id: 'f1' });
		await expect.element(page.getByText('overflowing')).toBeVisible();
		expect(facts()).toEqual([
			{ term: m.feeder_food_level(), value: 'overflowing', warn: false },
			{ term: m.feeder_battery(), value: percent(0.9), warn: false },
			{ term: m.feeder_power_source(), value: m.feeder_power_mains(), warn: false },
			{ term: m.common_status(), value: m.feeder_feeding(), warn: false }
		]);
	});

	it('says when the status cannot be read', async () => {
		stubApi({ [STATUS]: new Response('{"error":"offline"}', { status: 503 }), [MEALS]: { success: true, decoded: [] } });
		await render(FeederControl, { id: 'f1' });
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
	});

	it('serves the portions chosen, the count in the button, and says when it served', async () => {
		const answer = deferred();
		const api = withStatus({ food_level: 'full' }, { 'POST /devices/f1/feeder/feed': () => answer.promise });
		await render(FeederControl, { id: 'f1' });
		const slider = page.getByRole('slider', { name: m.feeder_portions() });
		await slider.click();
		await userEvent.keyboard('{Home}{ArrowRight}{ArrowRight}');
		await expect.element(slider).toHaveAttribute('aria-valuetext', m.feeder_portion({ count: 3 }));
		await page.getByRole('button', { name: m.feeder_distribute({ count: 3 }) }).click();
		const busy = page.getByRole('button', { name: m.feeder_distributing() });
		await expect.element(busy).toBeDisabled();
		await expect.element(busy).toHaveAttribute('aria-busy', 'true');
		answer.resolve({ success: true });
		await expect.element(page.getByText(m.feeder_served_at({ time: '' }).trim(), { exact: false })).toBeVisible();
		expect(api.sent('POST', '/devices/f1/feeder/feed')[0].body).toEqual({ portion: 3 });
	});

	it('keeps the scheduled meals in the second tab', async () => {
		withStatus({});
		await render(FeederControl, { id: 'f1' });
		await page.getByRole('tab', { name: m.feeder_schedule() }).click();
		await expect.element(page.getByText(m.feeder_meal_schedule_description())).toBeVisible();
		await expect.element(page.getByText(m.meal_plan_no_meals())).toBeVisible();
	});
});
