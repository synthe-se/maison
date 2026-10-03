import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { MealPlanEntry } from '#lib/api.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi, type ApiCall } from '#lib/test/api.ts';
import MealPlanManager from './MealPlanManager.svelte';
import { describeDays } from './meals.ts';
import { clockOf } from './tuya.svelte.ts';

const PATH = '/devices/f1/feeder/meal-plan';
const breakfast: MealPlanEntry = { time: '08:00', portion: 2, days_of_week: ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday'], status: 'Enabled' };
const dinner: MealPlanEntry = { time: '19:30', portion: 3, days_of_week: ['Saturday', 'Sunday'], status: 'Enabled' };
const snack: MealPlanEntry = { time: '12:00', portion: 1, days_of_week: ['Wednesday'], status: 'Disabled' };

/** A feeder that keeps what it is sent, and answers it on the next read. */
function feeder(plan: MealPlanEntry[]) {
	let stored = plan;
	return stubApi({
		[`GET ${PATH}`]: () => ({ success: true, decoded: stored }),
		[`POST ${PATH}`]: (c: ApiCall) => {
			stored = (c.body as { meal_plan: MealPlanEntry[] }).meal_plan;
			return { success: true };
		}
	});
}
const posted = (api: ReturnType<typeof stubApi>) => api.sent('POST', PATH).map((c) => (c.body as { meal_plan: MealPlanEntry[] }).meal_plan);
const rows = () => page.getByRole('listitem');

describe('MealPlanManager', () => {
	afterEach(() => {
		forgetAll();
		vi.useRealTimers();
	});

	it('lists the meals by time, in words, with the portions a day', async () => {
		feeder([dinner, snack, breakfast]);
		await render(MealPlanManager, { id: 'f1' });
		await expect.element(rows().nth(0)).toMatchTextContent(clockOf('08:00'));
		await expect.element(rows().nth(0)).toMatchTextContent(describeDays(breakfast.days_of_week));
		await expect.element(rows().nth(1)).toMatchTextContent(clockOf('12:00'));
		await expect.element(rows().nth(2)).toMatchTextContent(m.feeder_portion({ count: 3 }));
		await expect.element(page.getByText(m.meal_plan_count({ count: 3 }))).toBeVisible();
		// the switched-off snack does not count
		await expect.element(page.getByText(m.meal_plan_total_portions_per_day({ total: m.feeder_portion({ count: 5 }) }))).toBeVisible();
		await expect.element(page.getByRole('switch', { name: m.meal_plan_meal({ time: clockOf('12:00') }) })).not.toBeChecked();
	});

	it('says when there is no meal yet', async () => {
		stubApi({ [`GET ${PATH}`]: { success: true, decoded: null } });
		await render(MealPlanManager, { id: 'f1' });
		await expect.element(page.getByText(m.meal_plan_no_meals())).toBeVisible();
		await expect.element(page.getByText(m.meal_plan_count({ count: 0 }))).toBeVisible();
	});

	it('switches a meal off at once, and says it is saved', async () => {
		const api = feeder([breakfast, dinner]);
		const say = vi.spyOn(ui, 'say');
		await render(MealPlanManager, { id: 'f1' });
		await page.getByRole('switch', { name: m.meal_plan_meal({ time: clockOf('19:30') }) }).click();
		await expect.poll(() => posted(api)).toEqual([[breakfast, { ...dinner, status: 'Disabled' }]]);
		await expect.poll(() => say).toHaveBeenCalledWith(m.meal_plan_meal_plan_saved());
		await expect.element(page.getByRole('switch', { name: m.meal_plan_meal({ time: clockOf('19:30') }) })).not.toBeChecked();
	});

	it('shows the plan being sent until the feeder answers, and is busy meanwhile', async () => {
		const answer = deferred();
		const api = stubApi({ [`GET ${PATH}`]: { success: true, decoded: [breakfast] }, [`POST ${PATH}`]: () => answer.promise });
		await render(MealPlanManager, { id: 'f1' });
		await page.getByRole('switch', { name: m.meal_plan_meal({ time: clockOf('08:00') }) }).click();
		await expect.element(page.getByRole('list')).toHaveAttribute('aria-busy', 'true');
		await expect.element(page.getByRole('switch')).not.toBeChecked();
		await expect.element(page.getByRole('button', { name: m.meal_plan_add_meal() })).toBeDisabled();
		answer.resolve({ success: true });
		await expect.element(page.getByRole('list')).toHaveAttribute('aria-busy', 'false');
		expect(posted(api)).toHaveLength(1);
	});

	it('goes back to the feeder’s plan when the change is refused', async () => {
		stubApi({ [`GET ${PATH}`]: { success: true, decoded: [breakfast] }, [`POST ${PATH}`]: new Response('{"error":"busy"}', { status: 500 }) });
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		await render(MealPlanManager, { id: 'f1' });
		const sw = page.getByRole('switch', { name: m.meal_plan_meal({ time: clockOf('08:00') }) });
		await sw.click();
		await expect.poll(() => fail).toHaveBeenCalledOnce();
		await expect.element(sw).toBeChecked();
	});

	it('deletes at once, with « Rétablir » to bring the meal back', async () => {
		const api = feeder([breakfast, dinner]);
		await render(MealPlanManager, { id: 'f1' });
		const time = clockOf('08:00');
		await page.getByRole('button', { name: m.meal_plan_delete_label({ time }) }).click();
		await expect.element(page.getByText(m.meal_plan_deleted({ time }))).toBeVisible();
		await expect.poll(() => posted(api)).toEqual([[dinner]]);
		await expect.element(rows()).toHaveLength(1);
		await page.getByRole('button', { name: m.meal_plan_restore() }).click();
		await expect.poll(() => posted(api)).toEqual([[dinner], [breakfast, dinner]]);
		await expect.element(page.getByText(m.meal_plan_deleted({ time }))).not.toBeInTheDocument();
		await expect.element(rows()).toHaveLength(2);
	});

	it('« Rétablir » takes the focus and is said; it stays while focused, then 10 s', async () => {
		const say = vi.spyOn(ui, 'say');
		feeder([breakfast]);
		await render(MealPlanManager, { id: 'f1' });
		await expect.element(rows()).toHaveLength(1);
		await page.getByRole('button', { name: m.meal_plan_delete_label({ time: clockOf('08:00') }) }).click();
		const restore = page.getByRole('button', { name: m.meal_plan_restore() });
		await expect.element(restore).toHaveFocus();
		await expect.poll(() => say.mock.calls.flat()).toContain(m.meal_plan_deleted_undo({ time: clockOf('08:00') }));
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		await vi.advanceTimersByTimeAsync(30_000);
		expect(restore.query()).not.toBeNull();
		(restore.element() as HTMLElement).blur();
		await vi.advanceTimersByTimeAsync(9_900);
		expect(restore.query()).not.toBeNull();
		await vi.advanceTimersByTimeAsync(100);
		expect(restore.query()).toBeNull();
	});

	it('« Rétablir » waits for a write in flight (two whole-plan writes would race)', async () => {
		const api = feeder([breakfast, dinner]);
		let release!: () => void;
		api.routes[`POST ${PATH}`] = () => new Promise((r) => (release = () => r({ success: true })));
		await render(MealPlanManager, { id: 'f1' });
		await page.getByRole('button', { name: m.meal_plan_delete_label({ time: clockOf('08:00') }) }).click();
		const restore = page.getByRole('button', { name: m.meal_plan_restore() });
		await expect.element(restore).toHaveAttribute('aria-disabled', 'true');
		(restore.element() as HTMLElement).click();
		release();
		await expect.element(restore).not.toHaveAttribute('aria-disabled');
		expect(posted(api)).toEqual([[dinner]]);
	});

	it('adds a meal from the sheet, and closes it', async () => {
		const api = feeder([dinner]);
		await render(MealPlanManager, { id: 'f1' });
		await page.getByRole('button', { name: m.meal_plan_add_meal() }).click();
		const sheet = page.getByRole('dialog', { name: m.meal_plan_add_meal() });
		await expect.element(sheet).toBeVisible();
		await sheet.getByLabelText(m.meal_plan_time()).fill('06:45');
		await sheet.getByRole('button', { name: m.common_save() }).click();
		await expect.element(sheet).not.toBeInTheDocument();
		await expect.poll(() => posted(api)).toEqual([[dinner, { ...breakfast, time: '06:45', portion: 1, days_of_week: expect.any(Array) }]]);
		expect(posted(api)[0][1].days_of_week).toHaveLength(7);
	});

	it('edits a meal in place', async () => {
		const api = feeder([breakfast, dinner]);
		await render(MealPlanManager, { id: 'f1' });
		await page.getByRole('button', { name: m.meal_plan_edit_label({ time: clockOf('19:30') }) }).click();
		const sheet = page.getByRole('dialog', { name: m.meal_plan_edit_meal() });
		await expect.element(sheet.getByLabelText(m.meal_plan_time())).toHaveValue('19:30');
		await sheet.getByLabelText(m.meal_plan_time()).fill('20:00');
		await sheet.getByRole('button', { name: m.common_save() }).click();
		await expect.poll(() => posted(api)).toEqual([[breakfast, { ...dinner, time: '20:00' }]]);
	});

	it('cancelling the sheet changes nothing', async () => {
		const api = feeder([breakfast]);
		await render(MealPlanManager, { id: 'f1' });
		await page.getByRole('button', { name: m.meal_plan_edit_label({ time: clockOf('08:00') }) }).click();
		await page.getByRole('button', { name: m.common_cancel() }).click();
		await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
		expect(posted(api)).toEqual([]);
	});

	it('stops at ten meals, and says so', async () => {
		feeder(Array.from({ length: 10 }, (_, i) => ({ ...breakfast, time: `0${i}:00` })));
		await render(MealPlanManager, { id: 'f1' });
		await expect.element(page.getByRole('button', { name: m.meal_plan_add_meal() })).toBeDisabled();
		await expect.element(page.getByText(m.meal_plan_limit())).toBeVisible();
	});
});
