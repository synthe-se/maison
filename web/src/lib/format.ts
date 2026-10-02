// Durations in the reader's words, one formatter for the whole app: « 45 s », « 3 min 20 s »,
// « 1 h 30 », « 2 j 4 h ». Two units at most; the units come from the messages.

import { m } from '#lib/paraglide/messages.js';

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A duration given in seconds. */
export function formatDuration(seconds: number): string {
	const t = Math.max(0, Math.round(seconds));
	if (t < MINUTE) return m.duration_seconds({ s: t });
	if (t < HOUR) {
		const min = Math.floor(t / MINUTE);
		const s = t % MINUTE;
		return s === 0 ? m.duration_minutes({ m: min }) : m.duration_minutes_seconds({ m: min, s });
	}
	if (t < DAY) {
		const h = Math.floor(t / HOUR);
		const min = Math.floor((t % HOUR) / MINUTE);
		return min === 0 ? m.duration_hours({ h }) : m.duration_hours_minutes({ h, m: min, mm: String(min).padStart(2, '0') });
	}
	const d = Math.floor(t / DAY);
	const h = Math.floor((t % DAY) / HOUR);
	return h === 0 ? m.duration_days({ d }) : m.duration_days_hours({ d, h });
}

/** A duration given in minutes (device counters). */
export function formatMinutes(minutes: number): string {
	return formatDuration(minutes * MINUTE);
}
