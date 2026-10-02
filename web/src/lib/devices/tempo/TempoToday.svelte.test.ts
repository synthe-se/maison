import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { TempoData, TempoPrediction } from '#lib/api.ts';
import { num, shortDay } from '#lib/i18n.svelte.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { dayWords, probable } from './colors.ts';
import TempoToday from './TempoToday.svelte';

const TARIFS = { blue: { hc: 0.1288, hp: 0.1552 }, white: { hc: 0.1447, hp: 0.1792 }, red: { hc: 0.1518, hp: 0.6586 }, dateDebut: '2025-08-01' };
const price = (eur: number) => m.tempo_price({ price: num(eur * 100, 2) });
const prediction = (date: string, predicted_color: TempoPrediction['predicted_color'], confidence: number): TempoPrediction => ({
	date,
	predicted_color,
	confidence,
	probabilities: { BLUE: 0, WHITE: 0, RED: 0, [predicted_color]: confidence },
	constraints: { can_be_red: true, can_be_white: true, is_in_red_period: true }
});
const data = (over: Partial<TempoData> = {}): TempoData => ({
	success: true,
	today: { date: '2026-12-10T00:00:00', color: 'RED' },
	tomorrow: { date: '2026-12-11T00:00:00', color: null },
	tarifs: TARIFS,
	message: '',
	...over
});
const at = (hhmm: string) => vi.setSystemTime(new Date(`2026-12-10T${hhmm}:00`));
const tomorrowLine = () => page.getByRole('listitem').filter({ hasText: m.day_tomorrow() });

describe('TempoToday', () => {
	beforeEach(() => vi.useFakeTimers({ toFake: ['Date'] }));
	afterEach(() => {
		vi.useRealTimers();
		forgetAll();
	});

	it('says today’s published colour, its day, and the day’s rate', async () => {
		at('14:00');
		stubApi({ '/tempo/predictions': { success: true, predictions: [] } });
		await render(TempoToday, { data: data() });
		const tile = page.getByRole('article', { name: m.day_today() });
		await expect.element(tile).toMatchTextContent(m.color_red());
		await expect.element(tile).toMatchTextContent(shortDay('2026-12-10'));
		await expect.element(page.getByText(m.tempo_current_tarif(), { exact: true })).toBeVisible();
		const prices = page.getByRole('definition');
		await expect.element(prices.nth(0)).toHaveTextContent(price(TARIFS.red.hc));
		await expect.element(prices.nth(1)).toHaveTextContent(price(TARIFS.red.hp));
	});

	it('before 10:30, says tomorrow is announced at 10:30, with the forecast', async () => {
		at('09:00');
		stubApi({ '/tempo/predictions': { success: true, predictions: [prediction('2026-12-11', 'WHITE', 0.4)] } });
		await render(TempoToday, { data: data() });
		await expect.element(tomorrowLine()).toMatchTextContent(`${m.tempo_announced_at()} · ${probable('WHITE', 0.4)}`);
	});

	it('before 10:30 without a forecast, says only when it will be known', async () => {
		at('10:29');
		stubApi({ '/tempo/predictions': { success: true, predictions: [] } });
		await render(TempoToday, { data: data() });
		await expect.element(tomorrowLine().getByText(m.tempo_announced_at(), { exact: true })).toBeVisible();
	});

	it('after 10:30, says the forecast alone, or « unknown »', async () => {
		at('10:30');
		const api = stubApi({ '/tempo/predictions': { success: true, predictions: [prediction('2026-12-11', 'BLUE', 0.7)] } });
		const view = await render(TempoToday, { data: data() });
		await expect.element(tomorrowLine().getByText(probable('BLUE', 0.7), { exact: true })).toBeVisible();
		await view.unmount();
		forgetAll();
		api.routes['/tempo/predictions'] = { success: true, predictions: [] };
		await render(TempoToday, { data: data() });
		await expect.element(tomorrowLine().getByText(m.common_unknown(), { exact: true })).toBeVisible();
	});

	it('says tomorrow’s colour once published', async () => {
		at('09:00');
		stubApi({ '/tempo/predictions': { success: true, predictions: [] } });
		await render(TempoToday, { data: data({ tomorrow: { date: '2026-12-11', color: 'BLUE' } }) });
		await expect.element(tomorrowLine().getByText(m.color_blue(), { exact: true })).toBeVisible();
		await expect.element(tomorrowLine()).toMatchTextContent(shortDay('2026-12-11'));
	});

	it('shows a forecast for an unpublished today, its rate estimated', async () => {
		at('08:00');
		stubApi({ '/tempo/predictions': { success: true, predictions: [prediction('2026-12-10', 'WHITE', 0.55)] } });
		await render(TempoToday, { data: data({ today: { date: '2026-12-10', color: null } }) });
		const tile = page.getByRole('article', { name: m.day_today() });
		await expect.element(tile.getByText(dayWords('WHITE', true, 0.55), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tempo_estimated(), { exact: false })).toBeVisible();
		await expect.element(page.getByRole('definition').nth(1)).toHaveTextContent(price(TARIFS.white.hp));
	});

	it('says « unknown » for a today nobody knows, without a rate', async () => {
		at('08:00');
		stubApi({ '/tempo/predictions': { success: true, predictions: [] } });
		await render(TempoToday, { data: data({ today: undefined, tarifs: null }) });
		await expect.element(page.getByRole('article', { name: m.day_today() }).getByText(m.common_unknown(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tempo_current_tarif())).not.toBeInTheDocument();
		await expect.element(page.getByText(m.tempo_all_tarifs())).not.toBeInTheDocument();
	});

	it('lists up to four forecasts after tomorrow', async () => {
		at('12:00');
		const days = ['2026-12-11', '2026-12-12', '2026-12-13', '2026-12-14', '2026-12-15', '2026-12-16'];
		stubApi({ '/tempo/predictions': { success: true, predictions: days.map((d) => prediction(d, 'BLUE', 0.8)) } });
		await render(TempoToday, { data: data() });
		await expect.element(page.getByText(m.tempo_prediction_dashboard_title())).toBeVisible();
		for (const d of days.slice(1, 5)) await expect.element(page.getByText(shortDay(d), { exact: true })).toBeVisible();
		await expect.element(page.getByText(shortDay('2026-12-16'), { exact: true })).not.toBeInTheDocument();
	});

	it('tells every rate in a table, and that the data is cached', async () => {
		at('12:00');
		stubApi({ '/tempo/predictions': { success: true, predictions: [] } });
		await render(TempoToday, { data: data({ cached: true }) });
		await page.getByText(m.tempo_all_tarifs()).click();
		const rows = page.getByRole('table').getByRole('row');
		await expect.element(rows).toHaveLength(4);
		await expect.element(page.getByRole('rowheader', { name: m.color_white() })).toBeVisible();
		await expect.element(rows.nth(3)).toMatchTextContent(price(TARIFS.red.hp));
		await expect.element(page.getByText(m.tempo_cached())).toBeVisible();
	});
});
