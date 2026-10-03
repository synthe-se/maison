import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { date, dayLabel, percent } from '#lib/i18n.svelte.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { tempoForecast } from '#lib/test/tempo.ts';
import TempoPage from './+page.svelte';

const site = (more: Record<string, unknown> = {}) =>
	stubApi({
		'/tempo/forecast': tempoForecast(),
		'/tempo/calendar?season=2026-2027': { success: true, season: '2026-2027', calendar: [], stock: tempoForecast().stock },
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
		site({ '/tempo/forecast': () => new Promise(() => {}) });
		await render(TempoPage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByText(m.common_loading()).first()).toBeVisible();
	});

	it('shows the calendar, the season’s days used and left, and the week', async () => {
		site();
		await render(TempoPage);
		await expect.element(page.getByRole('heading', { name: m.tempo_calendar_title() })).toBeVisible();
		const stock = page.getByRole('region', { name: m.tempo_season({ season: '2026-2027' }) });
		await expect.element(stock).toMatchTextContent(m.tempo_days_used({ count: 4, total: 22 }));
		await expect.element(stock).toMatchTextContent(m.tempo_left({ count: 18, total: 22 }));
		await expect.element(stock).toMatchTextContent(m.tempo_left({ count: 205, total: 300 }));
		const week = page.getByRole('region', { name: m.tempo_prediction_week_forecast() });
		await expect.element(week.getByRole('article')).toHaveLength(3);
		await expect.element(week.getByRole('article', { name: dayLabel('2026-12-12') })).toBeVisible();
	});

	it('says the forecast is Maison’s, not official, with its measured reliability by horizon', async () => {
		site();
		await render(TempoPage);
		await expect.element(page.getByRole('heading', { name: m.tempo_unofficial_title() })).toBeVisible();
		const table = page.getByRole('table', { name: m.tempo_reliability_title() });
		await expect.element(table.getByRole('row')).toHaveLength(8);
		const horizon = tempoForecast().model!.backtest.horizons[1];
		await expect.element(table.getByRole('row').filter({ hasText: m.tempo_horizon_day({ n: 2 }) })).toMatchTextContent(percent(horizon.accuracy));
		await expect.element(page.getByText(m.tempo_reliability_hint({ seasons: '2024-2025, 2025-2026', blue: percent(0.818) }))).toBeVisible();
	});

	it('says when the weather forecast is old, and when the forecast stops short', async () => {
		site({ '/tempo/forecast': tempoForecast({ stale: true, weather_issued: '2026-12-08', note: 'the weather forecast does not reach J+7' }) });
		await render(TempoPage);
		await expect.element(page.getByText(m.tempo_stale({ date: date(new Date('2026-12-08T12:00:00'), { day: 'numeric', month: 'long' }) }))).toBeVisible();
		await expect.element(page.getByText(m.tempo_short())).toBeVisible();
	});

	it('says the load failed, with a retry, and keeps the calendar', async () => {
		site({ '/tempo/forecast': new Response('{"success":false,"error":"Tempo service temporarily unavailable"}', { status: 503 }) });
		await render(TempoPage);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
		await expect.element(page.getByRole('heading', { name: m.tempo_calendar_title() })).toBeVisible();
	});
});
