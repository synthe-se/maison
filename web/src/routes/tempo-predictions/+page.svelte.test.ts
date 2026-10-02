import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { TempoPrediction } from '#lib/api.ts';
import { dayLabel } from '#lib/i18n.svelte.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import TempoPage from './+page.svelte';

const prediction = (date: string): TempoPrediction => ({
	date,
	predicted_color: 'BLUE',
	confidence: 0.8,
	probabilities: { BLUE: 0.8, WHITE: 0.15, RED: 0.05 },
	constraints: { can_be_red: true, can_be_white: true, is_in_red_period: true }
});
const state = { season: '2026-2027', stock_red_remaining: 18, stock_red_total: 22, stock_white_remaining: 43, stock_white_total: 43 };
const forecasts = { success: true, predictions: [prediction('2026-12-11'), prediction('2026-12-12')], state, model_version: '2.1', message: '' };
const site = (more: Record<string, unknown> = {}) =>
	stubApi({
		'/tempo/predictions': forecasts,
		'/tempo/state': { success: true, ...state, message: '' },
		'/tempo/calendar?season=2026-2027': { success: true, calendar: [] },
		...more
	});

describe('Tempo page', () => {
	beforeEach(() => {
		vi.useFakeTimers({ toFake: ['Date'] });
		vi.setSystemTime(new Date('2026-12-10T12:00:00'));
	});
	afterEach(() => {
		vi.useRealTimers();
		forgetAll();
	});

	it('says it is loading', async () => {
		site({ '/tempo/predictions': () => new Promise(() => {}) });
		await render(TempoPage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.tempo_prediction_title() })).toBeVisible();
		await expect.element(page.getByRole('status')).toHaveTextContent(m.common_loading());
	});

	it('shows the calendar, the season’s stock, the week’s forecasts and the model', async () => {
		site();
		await render(TempoPage);
		await expect.element(page.getByRole('heading', { name: m.tempo_calendar_title() })).toBeVisible();
		const stock = page.getByRole('region', { name: m.tempo_season({ season: '2026-2027' }) });
		await expect.element(stock).toMatchTextContent(m.tempo_stock_used({ count: 4, total: 22 }));
		await expect.element(stock).toMatchTextContent(m.tempo_stock_remaining({ count: 18 }));
		await expect.element(stock).toMatchTextContent(m.tempo_stock_used({ count: 0, total: 43 }));
		const week = page.getByRole('region', { name: m.tempo_prediction_week_forecast() });
		await expect.element(week.getByRole('article')).toHaveLength(2);
		await expect.element(week.getByRole('article', { name: dayLabel('2026-12-11') })).toBeVisible();
		await expect.element(page.getByText(m.tempo_model({ version: '2.1' }))).toBeVisible();
		await expect.element(page.getByRole('heading', { name: m.tempo_prediction_how_it_works() })).toBeVisible();
	});

	it('goes without the stock and the week when the server has none', async () => {
		site({ '/tempo/predictions': { success: true, message: '' } });
		await render(TempoPage);
		await expect.element(page.getByRole('heading', { name: m.tempo_calendar_title() })).toBeVisible();
		await expect.element(page.getByRole('heading', { name: m.tempo_prediction_week_forecast() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('heading', { name: m.tempo_season({ season: '2026-2027' }) })).not.toBeInTheDocument();
	});

	it('says the server’s error when the forecasts fail, and how to fix it', async () => {
		site({ '/tempo/predictions': { success: false, error: 'Prediction server down', message: '' } });
		await render(TempoPage);
		await expect.element(page.getByRole('alert')).toMatchTextContent('Prediction server down');
		await expect.element(page.getByRole('alert')).toMatchTextContent(m.tempo_prediction_error_hint());
		await expect.element(page.getByRole('heading', { name: m.tempo_calendar_title() })).not.toBeInTheDocument();
	});

	it('says a generic error when the request itself fails', async () => {
		site({ '/tempo/predictions': new Error('network down') });
		await render(TempoPage);
		await expect.element(page.getByRole('alert')).toMatchTextContent(m.tempo_prediction_error());
	});

	it('says the state’s error when only the state fails', async () => {
		site({ '/tempo/state': { success: false, error: 'No state', message: '' } });
		await render(TempoPage);
		await expect.element(page.getByRole('alert')).toMatchTextContent('No state');
	});
});
