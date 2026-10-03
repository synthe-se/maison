import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { clock, num } from '#lib/i18n.svelte.ts';
import { time } from '#lib/clock.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { TARIFFS, forecastDay, tempoForecast, tempoToday } from '#lib/test/tempo.ts';
import { probable } from './colors.ts';
import TempoNow from './TempoNow.svelte';

const price = (eur: number) => m.tempo_cents({ cents: num(eur * 100, 1) });
/** The forecast without RTE's tomorrow: what the server says before 10:40. */
const before = (confidence = 0.4) =>
	tempoForecast({ days: [forecastDay('2026-12-11', 1, 'WHITE', confidence), forecastDay('2026-12-12', 2, 'RED', 0.62)] });
const at = (hhmm: string) => (time.now = new Date(`2026-12-10T${hhmm}:00`));
const h = (hour: number) => clock(new Date(2000, 0, 1, hour, 0));
const tile = () => page.getByRole('article');

describe('TempoNow', () => {
	afterEach(() => {
		time.now = new Date();
	});

	it('in two lines: the color and the price in force until when, then tomorrow; it leads to the Tempo page', async () => {
		at('14:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoNow, { data: tempoToday({ tomorrow: { date: '2026-12-11', color: 'BLUE' } }) });
		await expect
			.element(tile())
			.toMatchTextContent(m.tempo_in_force({ color: m.color_red(), price: `${m.tempo_hp()} ${price(TARIFFS.red.peak)}`, time: h(22) }));
		await expect.element(tile()).toMatchTextContent(m.tempo_tomorrow_is({ words: m.color_blue() }));
		await expect
			.element(
				page.getByRole('link', {
					name: m.tempo_in_force({ color: m.color_red(), price: `${m.tempo_hp()} ${price(TARIFFS.red.peak)}`, time: h(22) })
				})
			)
			.toHaveAttribute('href', '/tempo-predictions');
	});

	it('before 06:00 says it as it is: yesterday’s color still in force, today’s from 06:00', async () => {
		at('03:00');
		stubApi({ '/tempo/forecast': tempoForecast() });
		await render(TempoNow, { data: tempoToday() });
		await expect
			.element(tile())
			.toMatchTextContent(
				m.tempo_still({ color: m.color_white().toLocaleLowerCase(), price: `${m.tempo_hc()} ${price(TARIFFS.white.offPeak)}`, time: h(6) })
			);
		await expect.element(tile()).toMatchTextContent(m.tempo_from({ color: m.color_red(), time: h(6) }));
		// never « today red » next to a white off-peak price
		await expect
			.element(tile())
			.not.toMatchTextContent(
				m.tempo_in_force({ color: m.color_red(), price: `${m.tempo_hc()} ${price(TARIFFS.white.offPeak)}`, time: h(6) })
			);
	});

	it('before 10:40, says tomorrow is announced around 10:40, with the forecast', async () => {
		at('09:00');
		stubApi({ '/tempo/forecast': before() });
		await render(TempoNow, { data: tempoToday() });
		await expect
			.element(tile())
			.toMatchTextContent(m.tempo_tomorrow_is({ words: `${m.tempo_announced_at()} · ${probable('WHITE', 0.4)}` }));
	});

	it('after 10:40 and RTE late, the forecast alone, or « unknown »', async () => {
		at('10:40');
		const api = stubApi({ '/tempo/forecast': before(0.7) });
		const view = await render(TempoNow, { data: tempoToday() });
		await expect.element(tile()).toMatchTextContent(m.tempo_tomorrow_is({ words: probable('WHITE', 0.7) }));
		await view.unmount();
		api.routes['/tempo/forecast'] = tempoForecast({ days: [] });
		await render(TempoNow, { data: tempoToday() });
		await expect.element(tile()).toMatchTextContent(m.tempo_tomorrow_is({ words: m.common_unknown() }));
	});

	it('says « unknown » for a today nobody knows, without a price', async () => {
		at('08:00');
		stubApi({ '/tempo/forecast': tempoForecast({ days: [] }) });
		await render(TempoNow, { data: tempoToday({ today: { date: '2026-12-10', color: null }, tariffs: null }) });
		await expect.element(tile().getByText(m.common_unknown(), { exact: true })).toBeVisible();
	});
});
