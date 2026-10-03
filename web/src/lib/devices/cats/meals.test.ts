import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { DAYS, WEEKDAYS, WEEKEND, dayName, describeDays } from './meals.ts';

describe('meal days', () => {
	it('names a feeder day in the reader’s language', () => {
		expect(dayName('Monday', 'long')).toBe('lundi');
		expect(dayName('Sunday', 'short')).toBe('dim.');
	});

	it('says every day, weekdays and the weekend in one word, whatever the order', () => {
		expect(describeDays([...DAYS])).toBe(m.meal_plan_everyday());
		expect(describeDays([...DAYS].toReversed())).toBe('Tous les jours');
		expect(describeDays([...WEEKDAYS])).toBe(m.meal_plan_weekdays());
		expect(describeDays(['Sunday', 'Saturday'])).toBe('Week-end');
		expect(describeDays([...WEEKEND])).toBe(m.meal_plan_weekend());
	});

	it('lists other days in week order, short', () => {
		const said = describeDays(['Friday', 'Monday', 'Wednesday']);
		expect(said).toBe(new Intl.ListFormat('fr', { style: 'short' }).format(['lun.', 'mer.', 'ven.']));
		expect(said.indexOf('lun.')).toBeLessThan(said.indexOf('ven.'));
	});

	it('does not take six days for the whole week', () => {
		expect(describeDays(DAYS.slice(0, 6))).not.toBe(m.meal_plan_everyday());
	});
});
