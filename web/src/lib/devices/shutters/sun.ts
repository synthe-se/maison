// The sun schedule in words, once: the offsets one can pick, how each reads, and when the
// next scheduled move happens. The backend computes the times; this only says them.

import { m } from '#lib/paraglide/messages.js';
import type { Shutter, SunSchedule } from '#lib/api.ts';
import { formatMinutes } from '#lib/format.ts';
import { clock, dayLabel, isoDay } from '#lib/i18n.svelte.ts';

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

export const offsetOptions = () => OFFSETS.map((v) => ({ value: String(v), label: offsetLabel(v) }));

/** « demain à 07:43 », « aujourd’hui à 20:24 », « vendredi à 07:45 ». */
export function dayAndTime(iso: string): string {
	const at = new Date(iso);
	return m.shutters_day_time({ day: dayLabel(isoDay(at)).toLowerCase(), time: clock(at) });
}

/** The tile's one fact: the next scheduled move, if any (« Fermeture 20:24 »). */
export function nextMove(c: Shutter): string | undefined {
	const moves = [
		c.nextOpen && { at: c.nextOpen, say: m.shutters_fact_open },
		c.nextClose && { at: c.nextClose, say: m.shutters_fact_close }
	].filter((x): x is { at: string; say: typeof m.shutters_fact_open } => !!x);
	const first = moves.sort((a, b) => a.at.localeCompare(b.at))[0];
	return first && first.say({ time: clock(new Date(first.at)) });
}

/** The schedule with one event turned on or off (a newly enabled one gets its default offset). */
export function toggled(s: SunSchedule, event: 'sunrise' | 'sunset', on: boolean): SunSchedule {
	return event === 'sunrise'
		? { ...s, openAtSunrise: on, sunriseOffsetMin: on && !s.openAtSunrise ? DEFAULT_OFFSET.sunrise : s.sunriseOffsetMin }
		: { ...s, closeAtSunset: on, sunsetOffsetMin: on && !s.closeAtSunset ? DEFAULT_OFFSET.sunset : s.sunsetOffsetMin };
}
