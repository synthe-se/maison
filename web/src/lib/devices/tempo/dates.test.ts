import { describe, expect, it } from 'vitest';
import { seasonOf, seasonOver } from './dates.ts';

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

describe('seasonOver', () => {
	it('ends a season on the first of September after it', () => {
		expect(seasonOver('2025-2026', new Date(2026, 7, 31, 23, 59))).toBe(false);
		expect(seasonOver('2025-2026', new Date(2026, 8, 1))).toBe(true);
		expect(seasonOver('2026-2027', new Date(2026, 9, 16))).toBe(false);
	});
});
