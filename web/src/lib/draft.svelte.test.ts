import { describe, expect, it } from 'vitest';
import { Draft } from './draft.svelte.ts';

describe('Draft', () => {
	it('is not changed when it opens; a change makes it so; undoing it does not', () => {
		const d = new Draft({ time: '08:00', days: ['Monday'] });
		expect(d.dirty).toBe(false);
		d.current.days.push('Friday');
		expect(d.dirty).toBe(true);
		d.current.days.pop();
		expect(d.dirty).toBe(false);
	});

	it('a copy: editing it never touches what it came from', () => {
		const meal = { time: '08:00', days: ['Monday'] };
		const d = new Draft(meal);
		d.current.time = '09:00';
		expect(meal.time).toBe('08:00');
	});

	it('reset: starts again from a value, not changed any more', () => {
		const d = new Draft({ label: '' });
		d.current.label = 'Accueil';
		d.reset({ label: 'Maison' });
		expect(d.current.label).toBe('Maison');
		expect(d.dirty).toBe(false);
	});
});
