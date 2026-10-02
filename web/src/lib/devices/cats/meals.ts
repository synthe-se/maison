// Days of a scheduled meal: the feeder speaks English day names, the reader sees theirs.

import { m } from '#lib/paraglide/messages.js';
import { locale, weekday } from '#lib/i18n.svelte.ts';

/** In week order, as the feeder writes them. */
export const DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'] as const;
export const WEEKDAYS = DAYS.slice(0, 5);
export const WEEKEND = DAYS.slice(5);

export const dayName = (day: string, form: 'long' | 'short') => weekday(DAYS.indexOf(day as (typeof DAYS)[number]), form);

const same = (a: readonly string[], b: readonly string[]) => a.length === b.length && b.every((d) => a.includes(d));

/** « Tous les jours », « En semaine », « Week-end », or the days themselves. */
export function describeDays(days: string[]): string {
	if (same(days, DAYS)) return m.meal_plan_everyday();
	if (same(days, WEEKDAYS)) return m.meal_plan_weekdays();
	if (same(days, WEEKEND)) return m.meal_plan_weekend();
	const ordered = DAYS.filter((d) => days.includes(d));
	return new Intl.ListFormat(locale(), { style: 'short' }).format(ordered.map((d) => dayName(d, 'short')));
}
