// Tempo on the dashboard in words, once (docs/ux.md § 8): the color and the price in force until
// when, then what comes next. Before 06:00 it says it as it is: yesterday's color is still in
// force (« Encore blanc jusqu’à 06:00 ») and today's starts at 06:00 (« Rouge à partir de
// 06:00 »), never « Aujourd’hui rouge » above an off-peak blue.

import { m } from '#lib/paraglide/messages.js';
import { hhmm, lower, num } from '#lib/i18n.svelte.ts';
import type { TempoForecast, TempoToday } from './api.ts';
import { TEMPO, dayWords } from './colors.ts';
import type { PriceNow } from './price.ts';

/** RTE publishes tomorrow's color around 10:40 (minutes after midnight). */
export const ANNOUNCED_AT = 10 * 60 + 40;

/** « 19,2 c€/kWh »: short enough for a row (the Tempo page says it to the ten-thousandth). */
export const cents = (eurPerKwh: number) => m.tempo_cents({ cents: num(eurPerKwh * 100, 1) });

/** Line 1: the color and the price in force until when. */
export function nowLine(data: TempoToday, current: PriceNow | null): string {
	if (!current) return dayWords(data.today.color, false);
	const price = `${current.period === 'peak' ? m.tempo_hp() : m.tempo_hc()} ${cents(current.price)}`;
	const time = hhmm(current.until);
	return current.dayStarted
		? m.tempo_in_force({ color: TEMPO[current.color].name(), price, time })
		: m.tempo_still({ color: lower(TEMPO[current.color].name()), price, time });
}

/** Line 2: what comes next. Before 06:00 today's color, from 06:00; after, tomorrow: RTE's,
 * else « annoncé vers 10:40 » (before then) and Maison's forecast; '' when nothing is known. */
export function nextLine(data: TempoToday, current: PriceNow | null, forecast: TempoForecast | undefined, now: Date): string {
	if (current && !current.dayStarted)
		return current.next ? m.tempo_from({ color: TEMPO[current.next].name(), time: hhmm(current.until) }) : '';
	if (data.tomorrow.color) return m.tempo_tomorrow_is({ words: dayWords(data.tomorrow.color, false) });
	const f = forecast?.days.find((d) => d.date === data.tomorrow.date);
	const parts = [
		now.getHours() * 60 + now.getMinutes() < ANNOUNCED_AT ? m.tempo_announced_at() : '',
		f ? dayWords(f.color, !f.official, f.confidence) : ''
	].filter(Boolean);
	return m.tempo_tomorrow_is({ words: parts.length ? parts.join(' · ') : dayWords(null, false) });
}
