import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { hhmm, lower } from '#lib/i18n.svelte.ts';
import { TARIFFS, forecastDay, tempoForecast, tempoToday } from '#lib/test/tempo.ts';
import { probable } from './colors.ts';
import { cents, nextLine, nowLine } from './now.ts';
import { priceNow } from './price.ts';

const at = (time: string) => new Date(`2026-12-10T${time}:00`);
const before = (confidence = 0.4) =>
	tempoForecast({ days: [forecastDay('2026-12-11', 1, 'WHITE', confidence), forecastDay('2026-12-12', 2, 'RED', 0.62)] });

describe('Tempo now, in words', () => {
	it('the color and the price in force until when, then tomorrow', () => {
		const data = tempoToday({ tomorrow: { date: '2026-12-11', color: 'BLUE' } });
		const p = priceNow(at('14:00'), data);
		expect(nowLine(data, p)).toBe(
			m.tempo_in_force({ color: m.color_red(), price: `${m.tempo_hp()} ${cents(TARIFFS.red.peak)}`, time: hhmm('22:00') })
		);
		expect(nextLine(data, p, undefined, at('14:00'))).toBe(m.tempo_tomorrow_is({ words: m.color_blue() }));
	});

	it('before 06:00: yesterday’s color still in force, today’s from 06:00', () => {
		const data = tempoToday();
		const p = priceNow(at('03:00'), data);
		expect(nowLine(data, p)).toBe(
			m.tempo_still({ color: lower(m.color_white()), price: `${m.tempo_hc()} ${cents(TARIFFS.white.offPeak)}`, time: hhmm('06:00') })
		);
		expect(nextLine(data, p, undefined, at('03:00'))).toBe(m.tempo_from({ color: m.color_red(), time: hhmm('06:00') }));
	});

	it('before 10:40, tomorrow is announced around 10:40, with the forecast; after, the forecast alone or « unknown »', () => {
		const data = tempoToday();
		expect(nextLine(data, priceNow(at('09:00'), data), before(), at('09:00'))).toBe(
			m.tempo_tomorrow_is({ words: `${m.tempo_announced_at()} · ${probable('WHITE', 0.4)}` })
		);
		expect(nextLine(data, priceNow(at('10:40'), data), before(0.7), at('10:40'))).toBe(
			m.tempo_tomorrow_is({ words: probable('WHITE', 0.7) })
		);
		expect(nextLine(data, priceNow(at('10:40'), data), tempoForecast({ days: [] }), at('10:40'))).toBe(
			m.tempo_tomorrow_is({ words: m.common_unknown() })
		);
	});

	it('today unknown, without a price: « unknown »', () => {
		const data = tempoToday({ today: { date: '2026-12-10', color: null }, tariffs: null });
		expect(nowLine(data, priceNow(at('08:00'), data))).toBe(m.common_unknown());
	});
});
