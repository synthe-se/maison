import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { dayFromToday, percent, shortDay } from '#lib/i18n.svelte.ts';
import { forecastDay } from '#lib/test/tempo.ts';
import { probable } from './colors.ts';
import ForecastCard from './ForecastCard.svelte';

describe('ForecastCard', () => {
	it('names the day and says the colour with its probability', async () => {
		const d = forecastDay(dayFromToday(1), 1, 'RED', 0.62);
		await render(ForecastCard, { d });
		const card = page.getByRole('article', { name: m.day_tomorrow() });
		await expect.element(card).toBeVisible();
		await expect.element(card.getByText(probable('RED', 0.62))).toBeVisible();
		await expect.element(card.getByText(shortDay(d.date))).toBeVisible();
	});

	it('lists the three probabilities cheapest first, each colour named, and the bar beside', async () => {
		const { container } = await render(ForecastCard, { d: forecastDay(dayFromToday(2), 2, 'RED', 0.62) });
		const items = page.getByRole('list', { name: m.tempo_probabilities() }).getByRole('listitem');
		await expect.element(items).toHaveLength(3);
		await expect.element(items.nth(0)).toMatchTextContent(m.color_blue());
		await expect.element(items.nth(0)).toMatchTextContent(percent(0.19));
		await expect.element(items.nth(2)).toMatchTextContent(m.color_red());
		await expect.element(items.nth(2)).toMatchTextContent(percent(0.62));
		const bar = container.querySelector('.bar')!;
		expect(bar.getAttribute('aria-hidden')).toBe('true');
		expect((bar.querySelector('.rouge') as HTMLElement).style.width).toBe('62%');
	});

	it('says how often it was right this far ahead', async () => {
		const d = forecastDay(dayFromToday(3), 3, 'BLUE', 0.45);
		await render(ForecastCard, { d });
		const text = m.tempo_reliability_day({ percent: percent(d.reliability!.accuracy), n: 3, winter: percent(d.reliability!.winter_accuracy) });
		await expect.element(page.getByText(text)).toBeVisible();
	});

	it('says RTE published the day, without probabilities', async () => {
		await render(ForecastCard, { d: forecastDay(dayFromToday(1), 1, 'WHITE', 1, true) });
		await expect.element(page.getByText(m.tempo_official())).toBeVisible();
		await expect.element(page.getByText(m.color_white(), { exact: true })).toBeVisible();
		await expect.element(page.getByRole('list', { name: m.tempo_probabilities() })).not.toBeInTheDocument();
	});
});
