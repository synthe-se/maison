import { describe, expect, it } from 'vitest';
import { NO_SUN_MS } from '#lib/paces.ts';
import { isSunMark, untilSun } from './api.ts';

const H = 3_600_000;

describe('untilSun', () => {
	it('asks again just after the next event, never sooner than a second', () => {
		expect(untilSun({ sunriseMs: 10 * H, sunsetMs: 4 * H, side: 'light' }, H)).toBe(3 * H + 5_000);
		expect(untilSun({ sunriseMs: H, sunsetMs: 4 * H, side: 'dark' }, H)).toBe(5_000);
		expect(untilSun({ sunriseMs: H - 60_000, sunsetMs: 4 * H, side: 'dark' }, H)).toBe(1_000);
	});

	it('asks again hourly when the sun says nothing: a place set later is followed', () => {
		expect(untilSun(null)).toBe(NO_SUN_MS);
		expect(untilSun({ sunriseMs: null, sunsetMs: null, side: null })).toBe(NO_SUN_MS);
	});
});

describe('isSunMark', () => {
	it('takes a side and a time, nothing else', () => {
		expect(isSunMark({ side: 'dark', untilMs: 1 })).toBe(true);
		expect(isSunMark({ side: 'dusk', untilMs: 1 })).toBe(false);
		expect(isSunMark({ side: 'dark' })).toBe(false);
		expect(isSunMark(null)).toBe(false);
	});
});
