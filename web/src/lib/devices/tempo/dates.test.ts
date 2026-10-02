import { describe, expect, it } from 'vitest';
import { seasonOf } from './dates.ts';

describe('seasonOf', () => {
	it('starts a season in September', () => {
		expect(seasonOf(2026, 8)).toBe('2026-2027');
		expect(seasonOf(2026, 11)).toBe('2026-2027');
	});

	it('counts January to August in the season begun the year before', () => {
		expect(seasonOf(2027, 0)).toBe('2026-2027');
		expect(seasonOf(2027, 7)).toBe('2026-2027');
	});
});
