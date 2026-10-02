import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { TempoPrediction } from '#lib/api.ts';
import { dayFromToday, dayLabel, percent, shortDay } from '#lib/i18n.svelte.ts';
import { probable } from './colors.ts';
import ForecastCard from './ForecastCard.svelte';

const forecast = (over: Partial<TempoPrediction> = {}): TempoPrediction => ({
	date: dayFromToday(1),
	predicted_color: 'RED',
	probabilities: { BLUE: 0.1, WHITE: 0.28, RED: 0.62 },
	confidence: 0.62,
	constraints: { can_be_red: true, can_be_white: true, is_in_red_period: true },
	...over
});

describe('ForecastCard', () => {
	it('names the day and says the colour with its probability', async () => {
		const p = forecast();
		await render(ForecastCard, { p });
		const card = page.getByRole('article', { name: m.day_tomorrow() });
		await expect.element(card).toBeVisible();
		await expect.element(card.getByText(probable('RED', 0.62))).toBeVisible();
		await expect.element(card.getByText(shortDay(p.date))).toBeVisible();
	});

	it('lists the three probabilities cheapest first, each colour named', async () => {
		await render(ForecastCard, { p: forecast() });
		const list = page.getByRole('list', { name: m.tempo_probabilities() });
		const items = list.getByRole('listitem');
		await expect.element(items).toHaveLength(3);
		await expect.element(items.nth(0)).toMatchTextContent(m.color_blue());
		await expect.element(items.nth(0)).toMatchTextContent(percent(0.1));
		await expect.element(items.nth(1)).toMatchTextContent(m.color_white());
		await expect.element(items.nth(1)).toMatchTextContent(percent(0.28));
		await expect.element(items.nth(2)).toMatchTextContent(m.color_red());
		await expect.element(items.nth(2)).toMatchTextContent(percent(0.62));
	});

	it('says what the rules allow that day', async () => {
		await render(ForecastCard, { p: forecast({ date: '2026-07-01', constraints: { can_be_red: false, can_be_white: false, is_in_red_period: false } }) });
		await expect.element(page.getByRole('heading', { name: dayLabel('2026-07-01') })).toBeVisible();
		await expect.element(page.getByText(m.tempo_prediction_outside_red_period())).toBeVisible();
		await expect.element(page.getByText(m.tempo_prediction_red_blocked())).toBeVisible();
		await expect.element(page.getByText(m.tempo_prediction_white_blocked())).toBeVisible();
	});

	it('says the red period, and no block when every colour is possible', async () => {
		await render(ForecastCard, { p: forecast() });
		await expect.element(page.getByText(m.tempo_prediction_red_period())).toBeVisible();
		await expect.element(page.getByText(m.tempo_prediction_red_blocked())).not.toBeInTheDocument();
		await expect.element(page.getByText(m.tempo_prediction_white_blocked())).not.toBeInTheDocument();
	});
});
