import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { TempoCalendar as Calendar } from '#lib/devices/tempo/api.ts';
import { tempoStock } from '#lib/test/tempo.ts';
import { date, longDay, percent, weekday } from '#lib/i18n.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi, type ApiCall } from '#lib/test/api.ts';
import { dayWords } from './colors.ts';
import TempoCalendar from './TempoCalendar.svelte';

const month = (y: number, mo: number) => date(new Date(y, mo, 1), { month: 'long', year: 'numeric' });
const season: Calendar = {
	success: true,
	season: '2026-2027',
	calendar: [
		{ date: '2026-12-01', color: 'BLUE', isActual: true, isPrediction: false },
		{ date: '2026-12-02', color: 'RED', isActual: true, isPrediction: false },
		{ date: '2026-12-29', color: 'WHITE', isActual: false, isPrediction: true, confidence: 0.47 }
	],
	stock: tempoStock({
		blue: { used: 1, total: 300, remaining: 299 },
		white: { used: 0, total: 43, remaining: 43 },
		red: { used: 1, total: 22, remaining: 21 }
	})
};
const table = () => page.getByRole('table');
/** The table is named after the month it shows. */
async function caption(y: number, mo: number) {
	await expect.element(table()).toHaveAccessibleName(month(y, mo));
}

describe('TempoCalendar', () => {
	let api: ReturnType<typeof stubApi>;
	beforeEach(() => {
		vi.useFakeTimers({ toFake: ['Date'] });
		vi.setSystemTime(new Date('2026-12-28T12:00:00'));
		api = stubApi({
			'/tempo/calendar?season=2026-2027': season,
			'/tempo/calendar?season=2025-2026': { success: true, season: '2025-2026', calendar: [], stock: tempoStock({ season: '2025-2026' }) }
		});
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	it('is a table named by its month, weekdays from Monday with their full names', async () => {
		await render(TempoCalendar);
		await caption(2026, 11);
		const headers = page.getByRole('columnheader');
		await expect.element(headers).toHaveLength(7);
		await expect.element(headers.nth(0)).toHaveTextContent(weekday(0, 'short'));
		await expect.element(headers.nth(0)).toHaveAttribute('abbr', weekday(0, 'long'));
		await expect.element(headers.nth(6)).toHaveAttribute('abbr', weekday(6, 'long'));
	});

	it('marks today, and says each day’s color in words', async () => {
		const { container } = await render(TempoCalendar);
		await expect.element(page.getByText(m.tempo_day_label({ date: longDay('2026-12-02'), state: m.color_red() }))).toBeInTheDocument();
		const current = container.querySelectorAll('[aria-current="date"]');
		expect(current).toHaveLength(1);
		expect(current[0].textContent).toContain(longDay('2026-12-28'));
		// a forecast day: its probability, on the cell and in words; under 60 %, « not sure »
		expect(container.querySelectorAll('.swatch.unsure')).toHaveLength(1);
		await expect
			.element(page.getByText(m.tempo_day_label({ date: longDay('2026-12-29'), state: dayWords('WHITE', true, 0.47) })))
			.toBeInTheDocument();
		await expect.element(page.getByText(percent(0.47), { exact: true })).toBeVisible();
		// December 2026 starts on a Tuesday: one empty cell before the 1st
		expect(container.querySelector('tbody tr')?.firstElementChild?.textContent).toBe('');
	});

	it('says the season’s counters in its legend', async () => {
		await render(TempoCalendar);
		await expect.element(page.getByText(m.tempo_legend_red({ count: 1, total: 22 }))).toBeVisible();
		await expect.element(page.getByText(m.tempo_legend_white({ count: 0, total: 43 }))).toBeVisible();
		await expect.element(page.getByText(m.tempo_legend_blue({ count: 1 }))).toBeVisible();
		await expect.element(page.getByText(m.tempo_legend_forecast({ count: 1 }))).toBeVisible();
	});

	it('names the colors alone while the counters are not known', async () => {
		api.routes['/tempo/calendar?season=2026-2027'] = () => new Promise(() => {});
		await render(TempoCalendar);
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
		await expect.element(page.getByText(m.color_red(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tempo_calendar_prediction(), { exact: true })).toBeVisible();
	});

	it('moves a month with its two buttons, no further than the forecasts reach, and back to today', async () => {
		const say = vi.spyOn(ui, 'say');
		await render(TempoCalendar);
		const next = page.getByRole('button', { name: m.tempo_next_month() });
		await next.click();
		await caption(2027, 0);
		expect(say).toHaveBeenCalledWith(month(2027, 0));
		await expect.element(next).toBeDisabled();
		await page.getByRole('button', { name: m.tempo_prev_month() }).click();
		await page.getByRole('button', { name: m.tempo_prev_month() }).click();
		await caption(2026, 10);
		const today = page.getByRole('button', { name: m.day_today() });
		await today.click();
		await caption(2026, 11);
		// still there, unavailable: pressed, it does not vanish under the focus
		await expect.element(today).toHaveAttribute('aria-disabled', 'true');
		await expect.element(today).toHaveFocus();
	});

	it('Page Up / Page Down move a month while the table has focus, Shift a year; a season change asks its days', async () => {
		await render(TempoCalendar);
		await expect.element(table()).toHaveAttribute('aria-keyshortcuts', 'PageUp PageDown Shift+PageUp Shift+PageDown');
		(table().element() as HTMLElement).focus();
		await userEvent.keyboard('{PageUp}');
		await caption(2026, 10);
		await userEvent.keyboard('{Shift>}{PageUp}{/Shift}');
		await caption(2025, 10);
		await expect.poll(() => api.calls.map((c: ApiCall) => c.path)).toContain('/tempo/calendar?season=2025-2026');
		await userEvent.keyboard('{Shift>}{PageDown}{/Shift}{PageDown}{PageDown}');
		await caption(2027, 0);
		// other keys are left to the page
		await userEvent.keyboard('{ArrowLeft}');
		await caption(2027, 0);
	});

	it('stops at the first season RTE’s history holds', async () => {
		await render(TempoCalendar);
		(table().element() as HTMLElement).focus();
		for (let i = 0; i < 14; i++) await userEvent.keyboard('{Shift>}{PageUp}{/Shift}');
		await caption(2014, 8);
		await expect.element(page.getByRole('button', { name: m.tempo_prev_month() })).toBeDisabled();
	});

	it('jumps to a season’s September from the season picker, and to this month for the current one', async () => {
		await render(TempoCalendar);
		await page.getByRole('button', { name: new RegExp(`^${m.tempo_season_label()}`) }).click();
		await page.getByRole('option', { name: m.tempo_season({ season: '2023-2024' }) }).click();
		await expect.element(table()).toHaveAccessibleName(month(2023, 8));
		await page.getByRole('button', { name: new RegExp(`^${m.tempo_season_label()}`) }).click();
		await page.getByRole('option', { name: m.tempo_season({ season: '2026-2027' }) }).click();
		await caption(2026, 11);
	});
});
