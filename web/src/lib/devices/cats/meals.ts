// Days of a scheduled meal: the feeder speaks English day names, the reader sees theirs.

import { m } from '#lib/paraglide/messages.js';
import { list, weekday } from '#lib/i18n.svelte.ts';
import type { MealPlanEntry } from './api.ts';

/** In week order, as the feeder writes them. */
export const DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'] as const;
export type Day = (typeof DAYS)[number];
export const WEEKDAYS = DAYS.slice(0, 5);
export const WEEKEND = DAYS.slice(5);

export const dayName = (day: string, form: 'long' | 'short') => weekday(DAYS.indexOf(day as Day), form);

const same = (a: readonly string[], b: readonly string[]) => a.length === b.length && b.every((d) => a.includes(d));

/** « Tous les jours », « En semaine », « Week-end », or the days themselves. */
export function describeDays(days: string[]): string {
	if (same(days, DAYS)) return m.meal_plan_everyday();
	if (same(days, WEEKDAYS)) return m.meal_plan_weekdays();
	if (same(days, WEEKEND)) return m.meal_plan_weekend();
	return list(DAYS.filter((d) => days.includes(d)).map((d) => dayName(d, 'short')));
}

/** A meal as its form holds it. */
export interface MealForm {
	time: string;
	portion: number;
	days: Day[];
	enabled: boolean;
}

/** A new meal starts at 08:00, one portion, every day, on. */
const NEW_MEAL: MealPlanEntry = { time: '08:00', portion: 1, daysOfWeek: [...DAYS], status: 'Enabled' };

/** The form for `meal` (a new one without). */
export function formOf(meal: MealPlanEntry = NEW_MEAL): MealForm {
	return {
		time: meal.time,
		portion: meal.portion,
		days: DAYS.filter((d) => meal.daysOfWeek.includes(d)),
		enabled: meal.status !== 'Disabled'
	};
}
