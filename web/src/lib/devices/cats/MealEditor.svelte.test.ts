import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { MealPlanEntry } from '#lib/devices/cats/api.ts';
import { ui } from '#lib/ui.svelte.ts';
import { Draft } from '#lib/draft.svelte.ts';
import MealEditor from './MealEditor.svelte';
import { DAYS, dayName, formOf } from './meals.ts';

const day = (d: string) => page.getByRole('button', { name: dayName(d, 'long'), exact: true });
const save = () => page.getByRole('button', { name: m.common_save() });

async function open(meal?: MealPlanEntry) {
	const onsave = vi.fn();
	const oncancel = vi.fn();
	// as the plan opens it: a draft of the meal
	const draft = new Draft(formOf(meal));
	await render(MealEditor, { draft, onsave, oncancel });
	return { onsave, oncancel, draft };
}

describe('MealEditor', () => {
	it('starts a new meal at 08:00, one portion, every day, active', async () => {
		const { onsave } = await open();
		await expect.element(page.getByLabelText(m.meal_plan_time())).toHaveValue('08:00');
		for (const d of DAYS) await expect.element(day(d)).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('slider')).toHaveAttribute('aria-valuetext', m.feeder_portion({ count: 1 }));
		await expect.element(page.getByRole('switch', { name: m.meal_plan_enable_meal() })).toBeChecked();
		await save().click();
		expect(onsave).toHaveBeenCalledWith({ time: '08:00', portion: 1, daysOfWeek: [...DAYS], status: 'Enabled' });
	});

	it('is filled from the meal being edited', async () => {
		const { onsave } = await open({ time: '18:30', portion: 3, daysOfWeek: ['Saturday', 'Sunday'], status: 'Disabled' });
		await expect.element(page.getByLabelText(m.meal_plan_time())).toHaveValue('18:30');
		await expect.element(day('Monday')).toHaveAttribute('aria-pressed', 'false');
		await expect.element(day('Sunday')).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('switch', { name: m.meal_plan_enable_meal() })).not.toBeChecked();
		await save().click();
		expect(onsave).toHaveBeenCalledWith({ time: '18:30', portion: 3, daysOfWeek: ['Saturday', 'Sunday'], status: 'Disabled' });
	});

	it('works from the keyboard: arrows move the portions and go through the days, Space toggles one, Enter saves', async () => {
		const { onsave } = await open();
		await page.getByRole('slider').click();
		await userEvent.keyboard('{Home}{ArrowRight}{ArrowRight}');
		await expect.element(page.getByRole('slider')).toHaveAttribute('aria-valuetext', m.feeder_portion({ count: 3 }));
		(day('Wednesday').element() as HTMLElement).focus();
		await userEvent.keyboard(' ');
		await expect.element(day('Wednesday')).toHaveAttribute('aria-pressed', 'false');
		await userEvent.keyboard('{ArrowRight}{Enter}');
		await expect.element(day('Thursday')).toHaveFocus();
		await expect.element(day('Thursday')).toHaveAttribute('aria-pressed', 'false');
		await userEvent.keyboard('{Enter}');
		await expect.element(day('Thursday')).toHaveAttribute('aria-pressed', 'true');
		(page.getByLabelText(m.meal_plan_time()).element() as HTMLInputElement).focus();
		await userEvent.keyboard('{Enter}');
		expect(onsave).toHaveBeenCalledWith({
			time: '08:00',
			portion: 3,
			daysOfWeek: ['Monday', 'Tuesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'],
			status: 'Enabled'
		});
	});

	it('keeps the days in week order, whatever the order they were picked in', async () => {
		const { onsave } = await open({ time: '07:00', portion: 1, daysOfWeek: ['Friday'], status: 'Enabled' });
		await day('Monday').click();
		await save().click();
		expect(onsave.mock.calls[0][0].daysOfWeek).toEqual(['Monday', 'Friday']);
	});

	it('picks every day, weekdays or the weekend in one press', async () => {
		const { onsave } = await open();
		await page.getByRole('button', { name: m.meal_plan_weekend(), exact: true }).click();
		await expect.element(day('Friday')).toHaveAttribute('aria-pressed', 'false');
		await expect.element(day('Saturday')).toHaveAttribute('aria-pressed', 'true');
		await page.getByRole('button', { name: m.meal_plan_weekdays_only(), exact: true }).click();
		await expect.element(day('Saturday')).toHaveAttribute('aria-pressed', 'false');
		await save().click();
		expect(onsave.mock.calls[0][0].daysOfWeek).toEqual(['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday']);
		await page.getByRole('button', { name: m.common_all(), exact: true }).click();
		await expect.element(day('Sunday')).toHaveAttribute('aria-pressed', 'true');
	});

	it('refuses a meal on no day, says why, and the error leaves with the next day picked', async () => {
		const say = vi.spyOn(ui, 'say');
		const { onsave } = await open({ time: '07:00', portion: 1, daysOfWeek: ['Friday'], status: 'Enabled' });
		await day('Friday').click();
		await save().click();
		expect(onsave).not.toHaveBeenCalled();
		await expect.element(page.getByText(m.meal_plan_select_at_least_one_day())).toBeVisible();
		await expect
			.element(page.getByRole('group', { name: m.meal_plan_days() }))
			.toHaveAccessibleDescription(m.meal_plan_select_at_least_one_day());
		expect(say).toHaveBeenCalledWith(m.meal_plan_select_at_least_one_day());
		await day('Monday').click();
		await expect.element(page.getByText(m.meal_plan_select_at_least_one_day())).not.toBeInTheDocument();
	});

	it('switches a meal off, and cancels without saving', async () => {
		const { onsave, oncancel } = await open();
		await page.getByRole('switch', { name: m.meal_plan_enable_meal() }).click();
		await save().click();
		expect(onsave.mock.calls[0][0].status).toBe('Disabled');
		await page.getByRole('button', { name: m.common_cancel() }).click();
		expect(oncancel).toHaveBeenCalledOnce();
	});

	it('a change marks the draft changed (the sheet asks before losing it); undoing it does not', async () => {
		const { draft } = await open({ time: '07:00', portion: 1, daysOfWeek: ['Friday'], status: 'Enabled' });
		expect(draft.dirty).toBe(false);
		await day('Monday').click();
		expect(draft.dirty).toBe(true);
		await day('Monday').click();
		expect(draft.dirty).toBe(false);
	});

	it('takes the time typed', async () => {
		const { onsave } = await open();
		await page.getByLabelText(m.meal_plan_time()).fill('21:15');
		await save().click();
		expect(onsave.mock.calls[0][0].time).toBe('21:15');
	});
});
