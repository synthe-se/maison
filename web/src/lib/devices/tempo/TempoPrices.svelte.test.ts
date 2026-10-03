import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { num } from '#lib/i18n.svelte.ts';
import { time } from '#lib/clock.svelte.ts';
import { TARIFFS, tempoToday } from '#lib/test/tempo.ts';
import TempoPrices from './TempoPrices.svelte';

const price = (eur: number) => m.tempo_price({ price: num(eur, 4) });
const lower = (name: () => string) => name().toLocaleLowerCase();
const period = (p: 'HP' | 'HC', color: () => string) =>
	m.tempo_period_color({ period: p === 'HP' ? m.tempo_hp() : m.tempo_hc(), color: lower(color) });
const at = (hhmm: string) => (time.now = new Date(`2026-12-10T${hhmm}:00`));

describe('TempoPrices', () => {
	afterEach(() => (time.now = new Date()));

	it('says the price in force with the other period’s', async () => {
		at('14:00');
		await render(TempoPrices, { data: tempoToday() });
		await expect.element(page.getByText(m.tempo_now(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(`${period('HP', m.color_red)} · ${price(TARIFFS.red.peak)}`)).toBeVisible();
		await expect.element(page.getByText(`${period('HC', m.color_red)} · ${price(TARIFFS.red.offPeak)}`)).toBeVisible();
	});

	it('before 06:00, the price in force is yesterday’s color, off-peak', async () => {
		at('03:00');
		await render(TempoPrices, { data: tempoToday() });
		await expect.element(page.getByText(`${period('HC', m.color_white)} · ${price(TARIFFS.white.offPeak)}`)).toBeVisible();
	});

	it('says tomorrow’s prices once its color is published', async () => {
		at('12:00');
		await render(TempoPrices, { data: tempoToday({ tomorrow: { date: '2026-12-11', color: 'BLUE' } }) });
		await expect
			.element(
				page.getByText(
					m.tempo_tomorrow_prices({ color: lower(m.color_blue), hc: price(TARIFFS.blue.offPeak), hp: price(TARIFFS.blue.peak) })
				)
			)
			.toBeVisible();
	});

	it('tells every rate in a table, since when, the subscription, and that the colors are old', async () => {
		at('12:00');
		await render(TempoPrices, { data: tempoToday({ cached: true }) });
		const rows = page.getByRole('table').getByRole('row');
		await expect.element(rows).toHaveLength(4);
		await expect.element(page.getByRole('rowheader', { name: m.color_white() })).toBeVisible();
		await expect.element(rows.nth(3)).toMatchTextContent(price(TARIFFS.red.peak));
		await expect.element(page.getByText(m.tempo_subscription({ price: num(189.98, 2) }), { exact: false })).toBeVisible();
		await expect.element(page.getByText(m.tempo_cached())).toBeVisible();
	});

	it('without prices, no rate and no table', async () => {
		at('12:00');
		await render(TempoPrices, { data: tempoToday({ tariffs: null }) });
		await expect.element(page.getByText(m.tempo_now())).not.toBeInTheDocument();
		await expect.element(page.getByRole('table')).not.toBeInTheDocument();
	});
});
