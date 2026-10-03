// The sun schedule in words, once: the offsets one can pick, how each reads, and when the
// next scheduled move happens. The backend computes the times; this only says them.

import { m } from '#lib/paraglide/messages.js';
import type { Shutter, SunSchedule } from './api.ts';
import { formatMinutes } from '#lib/format.ts';
import { clock, dayFromToday, dayLabel, isoDay, lower } from '#lib/i18n.svelte.ts';

/** Minutes before (negative) or after the sun: quarter hours near it, then half hours, ± 3 h. */
export const OFFSETS = [-180, -120, -90, -60, -45, -30, -15, 0, 15, 30, 45, 60, 90, 120, 180] as const;

/** What a newly enabled event starts with: closing an hour after sunset (Leonard's choice). */
export const DEFAULT_OFFSET = { sunrise: 0, sunset: 60 } as const;

/** « À l’heure du soleil », « 1 h après », « 30 min avant ». */
export function offsetLabel(minutes: number): string {
	if (minutes === 0) return m.shutters_offset_on_time();
	const duration = formatMinutes(Math.abs(minutes));
	return minutes > 0 ? m.shutters_offset_after({ duration }) : m.shutters_offset_before({ duration });
}

/** The offsets as a picker's values (« -30 »). */
export const OFFSET_VALUES = OFFSETS.map(String);

/** « demain à 07:43 », « aujourd’hui à 20:24 », « vendredi à 07:45 ». */
export function dayAndTime(iso: string): string {
	const at = new Date(iso);
	return m.shutters_day_time({ day: lower(dayLabel(isoDay(at))), time: clock(at) });
}

export type SunEvent = 'open' | 'close';

/** The next scheduled move, if any: which, when (ISO), and whether it will be skipped once. */
export function nextEvent(c: Shutter): { event: SunEvent; at: string; skipped: boolean } | undefined {
	const moves = [
		c.nextOpen ? { event: 'open' as const, at: c.nextOpen, skipped: c.skipNextOpen } : null,
		c.nextClose ? { event: 'close' as const, at: c.nextClose, skipped: c.skipNextClose } : null
	].filter((x) => x !== null);
	return moves.toSorted((a, b) => new Date(a.at).getTime() - new Date(b.at).getTime())[0];
}

/** The next move in words: « Fermeture 20:24 », or « Fermeture de 20:24 sautée ». */
export function nextMove(c: Shutter): string | undefined {
	const e = nextEvent(c);
	if (!e) return undefined;
	const time = clock(new Date(e.at));
	if (e.skipped) return e.event === 'open' ? m.shutters_open_skipped({ time }) : m.shutters_close_skipped({ time });
	return e.event === 'open' ? m.shutters_fact_open({ time }) : m.shutters_fact_close({ time });
}

/** « Ne pas fermer ce soir », « Ne pas ouvrir demain matin »: skipping the next move once. */
export function skipLabel(event: SunEvent, at: string): string {
	const day = isoDay(new Date(at));
	const today = dayFromToday(0);
	const tomorrow = dayFromToday(1);
	if (event === 'close')
		return day === today ? m.shutters_skip_close_tonight() : day === tomorrow ? m.shutters_skip_close_tomorrow() : m.shutters_skip_close();
	return day === today ? m.shutters_skip_open_today() : day === tomorrow ? m.shutters_skip_open_tomorrow() : m.shutters_skip_open();
}

/** « Volet salon se ferme à 18:42 »: the « Maintenant » strip's sentence. */
export function moveSentence(c: Shutter): string | undefined {
	const e = nextEvent(c);
	if (!e || e.skipped) return undefined;
	const time = clock(new Date(e.at));
	return e.event === 'open' ? m.shutters_will_open({ name: c.name, time }) : m.shutters_will_close({ name: c.name, time });
}

/** The schedule with one event turned on or off (a newly enabled one gets its default offset). */
export function toggled(s: SunSchedule, event: 'sunrise' | 'sunset', on: boolean): SunSchedule {
	return event === 'sunrise'
		? { ...s, openAtSunrise: on, sunriseOffsetMin: on && !s.openAtSunrise ? DEFAULT_OFFSET.sunrise : s.sunriseOffsetMin }
		: { ...s, closeAtSunset: on, sunsetOffsetMin: on && !s.closeAtSunset ? DEFAULT_OFFSET.sunset : s.sunsetOffsetMin };
}
