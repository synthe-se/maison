import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { num, shortDay } from '#lib/i18n.svelte.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { TARIFS, forecastDay, tempoForecast, tempoToday } from '#lib/test/tempo.ts';
import { probable } from './colors.ts';
import TempoToday from './TempoToday.svelte';

const price = (eur: number) => m.tempo_price({ price: num(eur, 4) });
const lower = (name: () => string) => name().toLocaleLowerCase();
const period = (p: 'HP' | 'HC', color: () => string) => m.tempo_period_color({ period: p === 'HP' ? m.tempo_hp() : m.tempo_hc(), color: lower(color) });
/** The forecast without RTE's tomorrow: what the server says before 10:40. */
const before = (confidence = 0.4) => tempoForecast({ days: [forecastDay('2026-12-11', 1, 'WHITE', confidence), forecastDay('2026-12-12', 2, 'RED', 0.62)] });
const at = (hhmm: string) => vi.setSystemTime(new Date(`2026-12-10T${hhmm}:00`));
const tomorrowLine = () => page.getByRole('listitem').filter({ hasText: m.day_tomorrow() });

describe('TempoToday', () => {
	beforeEach(() => vi.useFakeTimers({ toFake: ['Date'] }));
	afterEach(() => {
		vi.useRealTimers();
		forgetAll();
	});

	it('says today’s published colour, its day, and the price in force with the other period’s', async () => {
		at('14:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoToday, { data: tempoToday() });
		const tile = page.getByRole('article', { name: m.day_today() });
		await expect.element(tile).toMatchTextContent(m.color_red());
		await expect.element(tile).toMatchTextContent(shortDay('2026-12-10'));
		await expect.element(page.getByText(m.tempo_now(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(`${period('HP', m.color_red)} · ${price(TARIFS.red.hp)}`)).toBeVisible();
		await expect.element(page.getByText(`${period('HC', m.color_red)} · ${price(TARIFS.red.hc)}`)).toBeVisible();
	});

	it('before 06:00, the price in force is yesterday’s colour, off-peak', async () => {
		at('03:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoToday, { data: tempoToday() });
		await expect.element(page.getByText(`${period('HC', m.color_white)} · ${price(TARIFS.white.hc)}`)).toBeVisible();
	});

	it('says tomorrow’s prices once its colour is published', async () => {
		at('12:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoToday, { data: tempoToday({ tomorrow: { date: '2026-12-11', color: 'BLUE' } }) });
		await expect
			.element(page.getByText(m.tempo_tomorrow_prices({ color: lower(m.color_blue), hc: price(TARIFS.blue.hc), hp: price(TARIFS.blue.hp) })))
			.toBeVisible();
	});

	it('before 10:40, says tomorrow is announced around 10:40, with the forecast', async () => {
		at('09:00');
		stubApi({ '/tempo/forecast': before() });
		await render(TempoToday, { data: tempoToday() });
		await expect.element(tomorrowLine()).toMatchTextContent(`${m.tempo_announced_at()} · ${probable('WHITE', 0.4)}`);
	});

	it('before 10:40 without a forecast, says only when it will be known', async () => {
		at('10:39');
		stubApi({ '/tempo/forecast': tempoForecast({ days: [] }) });
		await render(TempoToday, { data: tempoToday() });
		await expect.element(tomorrowLine().getByText(m.tempo_announced_at(), { exact: true })).toBeVisible();
	});

	it('after 10:40 and RTE late, says the forecast alone, or « unknown »', async () => {
		at('10:40');
		const api = stubApi({ '/tempo/forecast': before(0.7) });
		const view = await render(TempoToday, { data: tempoToday() });
		await expect.element(tomorrowLine().getByText(probable('WHITE', 0.7), { exact: true })).toBeVisible();
		await view.unmount();
		forgetAll();
		api.routes['/tempo/forecast'] = tempoForecast({ days: [] });
		await render(TempoToday, { data: tempoToday() });
		await expect.element(tomorrowLine().getByText(m.common_unknown(), { exact: true })).toBeVisible();
	});

	it('says tomorrow’s colour once published', async () => {
		at('11:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoToday, { data: tempoToday({ tomorrow: { date: '2026-12-11', color: 'BLUE' } }) });
		await expect.element(tomorrowLine().getByText(m.color_blue(), { exact: true })).toBeVisible();
		await expect.element(tomorrowLine()).toMatchTextContent(shortDay('2026-12-11'));
	});

	it('lists the forecast after tomorrow, each day with its probability, unsure ones dotted', async () => {
		at('12:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		const { container } = await render(TempoToday, { data: tempoToday() });
		await expect.element(page.getByText(m.tempo_prediction_dashboard_title())).toBeVisible();
		await expect.element(page.getByText(probable('RED', 0.62), { exact: true })).toBeVisible();
		await expect.element(page.getByText(probable('BLUE', 0.45), { exact: true })).toBeVisible();
		await expect.element(page.getByText(shortDay('2026-12-13'), { exact: true })).toBeVisible();
		// tomorrow is RTE's: on its own line, not listed again
		await expect.element(page.getByText(shortDay('2026-12-11'), { exact: true })).toHaveLength(1);
		expect(container.querySelectorAll('.swatch.unsure')).toHaveLength(1);
	});

	it('says the days left of each colour', async () => {
		at('12:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoToday, { data: tempoToday() });
		const left = page.getByRole('listitem').filter({ hasText: m.tempo_left({ count: 18, total: 22 }) });
		await expect.element(left).toMatchTextContent(m.color_red());
		await expect.element(page.getByText(m.tempo_left({ count: 40, total: 43 }))).toBeVisible();
		await expect.element(page.getByText(m.tempo_left({ count: 205, total: 300 }))).toBeVisible();
	});

	it('says « unknown » for a today nobody knows, without a rate', async () => {
		at('08:00');
		stubApi({ '/tempo/forecast': tempoForecast({ days: [] }) });
		await render(TempoToday, { data: tempoToday({ today: { date: '2026-12-10', color: null }, tarifs: null }) });
		await expect.element(page.getByRole('article', { name: m.day_today() }).getByText(m.common_unknown(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tempo_now())).not.toBeInTheDocument();
		await expect.element(page.getByText(m.tempo_all_tarifs())).not.toBeInTheDocument();
	});

	it('tells every rate in a table, since when, the subscription, and that the colours are old', async () => {
		at('12:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoToday, { data: tempoToday({ cached: true }) });
		await page.getByText(m.tempo_all_tarifs()).click();
		const rows = page.getByRole('table').getByRole('row');
		await expect.element(rows).toHaveLength(4);
		await expect.element(page.getByRole('rowheader', { name: m.color_white() })).toBeVisible();
		await expect.element(rows.nth(3)).toMatchTextContent(price(TARIFS.red.hp));
		await expect.element(page.getByText(m.tempo_subscription({ price: num(189.98, 2) }), { exact: false })).toBeVisible();
		await expect.element(page.getByText(/2026/)).toBeVisible();
		await expect.element(page.getByText(m.tempo_cached())).toBeVisible();
	});
});
