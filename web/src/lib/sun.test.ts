import { describe, expect, it } from 'vitest';
import { sunSide, untilSun } from './sun.ts';

const H = 3_600_000;

describe('sunSide', () => {
	it('is dark when the sun rises next, light when it sets next', () => {
		expect(sunSide({ sunriseMs: 5 * H, sunsetMs: 17 * H })).toBe('dark');
		expect(sunSide({ sunriseMs: 29 * H, sunsetMs: 17 * H })).toBe('light');
	});

	it('says nothing without a place, and follows a polar day or night', () => {
		expect(sunSide(null)).toBeNull();
		expect(sunSide({ sunriseMs: null, sunsetMs: null })).toBeNull();
		expect(sunSide({ sunriseMs: null, sunsetMs: 2 * H })).toBe('light');
		expect(sunSide({ sunriseMs: 2 * H, sunsetMs: null })).toBe('dark');
	});
});

describe('untilSun', () => {
	it('asks again just after the next event, never sooner than a second', () => {
		expect(untilSun({ sunriseMs: 10 * H, sunsetMs: 4 * H }, H)).toBe(3 * H + 5_000);
		expect(untilSun({ sunriseMs: H, sunsetMs: 4 * H }, H)).toBe(5_000);
		expect(untilSun({ sunriseMs: H - 60_000, sunsetMs: 4 * H }, H)).toBe(1_000);
	});

	it('does not ask again when the sun says nothing', () => {
		expect(untilSun(null)).toBe(0);
		expect(untilSun({ sunriseMs: null, sunsetMs: null })).toBe(0);
	});
});
