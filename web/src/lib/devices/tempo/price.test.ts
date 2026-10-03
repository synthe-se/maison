import { describe, expect, it } from 'vitest';
import { TARIFFS, tempoToday } from '#lib/test/tempo.ts';
import { m } from '#lib/paraglide/messages.js';
import { costPerHour, euros, monthEstimate, peakShare, perHour, priceNow, redPeak } from './price.ts';

// yesterday white, today red
const at = (hhmm: string, day = '2026-12-10') => priceNow(new Date(`${day}T${hhmm}:00`), tempoToday());

describe('priceNow', () => {
	it('is today’s peak price from 06:00 to 21:59', () => {
		expect(at('06:00')).toEqual({
			color: 'RED',
			period: 'peak',
			price: TARIFFS.red.peak,
			until: '22:00',
			other: { period: 'offPeak', price: TARIFFS.red.offPeak },
			dayStarted: true,
			next: null
		});
		expect(at('21:59')).toMatchObject({ color: 'RED', period: 'peak', price: TARIFFS.red.peak });
	});

	it('is today’s off-peak price from 22:00 to midnight', () => {
		expect(at('22:00')).toEqual({
			color: 'RED',
			period: 'offPeak',
			price: TARIFFS.red.offPeak,
			until: '06:00',
			other: { period: 'peak', price: TARIFFS.red.peak },
			dayStarted: true,
			next: null
		});
		expect(at('23:59')).toMatchObject({ color: 'RED', period: 'offPeak' });
	});

	it('is still yesterday’s color until 05:59: a Tempo day runs 06:00 to 06:00', () => {
		expect(at('00:00')).toMatchObject({
			color: 'WHITE',
			period: 'offPeak',
			price: TARIFFS.white.offPeak,
			until: '06:00',
			dayStarted: false,
			next: 'RED'
		});
		expect(at('05:59')).toMatchObject({ color: 'WHITE', period: 'offPeak', price: TARIFFS.white.offPeak });
		expect(at('06:00')).toMatchObject({ color: 'RED', period: 'peak' });
	});

	it('says nothing without prices or without the color in force', () => {
		expect(priceNow(new Date('2026-12-10T12:00:00'), tempoToday({ tariffs: null }))).toBeNull();
		expect(priceNow(new Date('2026-12-10T03:00:00'), tempoToday({ yesterday: { date: '2026-12-09', color: null } }))).toBeNull();
	});

	it('follows the server’s hours', () => {
		const data = tempoToday({ hours: { peakStart: '07:00', peakEnd: '23:00' } });
		expect(priceNow(new Date('2026-12-10T06:30:00'), data)).toMatchObject({ color: 'WHITE', period: 'offPeak' });
		expect(priceNow(new Date('2026-12-10T22:30:00'), data)).toMatchObject({ color: 'RED', period: 'peak', until: '23:00' });
	});
});

describe('the cost of a device', () => {
	it('is its power times the price in force', () => {
		expect(costPerHour(42, 0.1654)).toBeCloseTo(0.006947, 6);
		expect(costPerHour(1040, 0.7295)).toBeCloseTo(0.7587, 4);
	});

	it('reads in cents under 10 c€/h, in euros above', () => {
		expect(perHour(0.006947)).toBe(m.price_cents_per_hour({ cents: '0,7' }));
		expect(perHour(0.7587)).toBe(m.price_euros_per_hour({ euros: '0,76' }));
		expect(euros(4.1234)).toBe(m.price_euros({ euros: '4,12' }));
	});

	it('flags the peak hours of a red day only', () => {
		const data = tempoToday();
		expect(redPeak(priceNow(new Date('2026-12-10T12:00:00'), data))).toBe(true);
		expect(redPeak(priceNow(new Date('2026-12-10T23:00:00'), data))).toBe(false);
		expect(redPeak(priceNow(new Date('2026-12-10T03:00:00'), data))).toBe(false);
		expect(redPeak(null)).toBe(false);
	});
});

describe('monthEstimate', () => {
	const hours = { peakStart: '06:00', peakEnd: '22:00' };

	it('takes 16 h of 24 as peak hours with 06:00–22:00', () => {
		expect(peakShare(hours)).toBeCloseTo(2 / 3, 6);
	});

	it('prices each day of the month at its color, 2/3 peak and 1/3 off-peak', () => {
		const colors: Record<string, 'BLUE' | 'RED'> = { '2026-12-01': 'BLUE', '2026-12-02': 'RED' };
		const days = [
			{ date: '2026-11-30', wh: 5000 },
			{ date: '2026-12-01', wh: 3000 },
			{ date: '2026-12-02', wh: 1500 },
			{ date: '2026-12-03', wh: 9000 }
		];
		const r = monthEstimate(days, (d) => colors[d], TARIFFS, hours, '2026-12');
		const blue = 3 * ((2 / 3) * TARIFFS.blue.peak + (1 / 3) * TARIFFS.blue.offPeak);
		const red = 1.5 * ((2 / 3) * TARIFFS.red.peak + (1 / 3) * TARIFFS.red.offPeak);
		// November is another month; the 3rd has no known color: both left out
		expect(r.days).toBe(2);
		expect(r.euros).toBeCloseTo(blue + red, 6);
	});
});
