import { describe, expect, it } from 'vitest';
import { TARIFS, tempoToday } from '#lib/test/tempo.ts';
import { priceNow } from './price.ts';

// yesterday white, today red
const at = (hhmm: string, day = '2026-12-10') => priceNow(new Date(`${day}T${hhmm}:00`), tempoToday());

describe('priceNow', () => {
	it('is today’s peak price from 06:00 to 21:59', () => {
		expect(at('06:00')).toEqual({ color: 'RED', period: 'hp', price: TARIFS.red.hp, until: '22:00', other: { period: 'hc', price: TARIFS.red.hc } });
		expect(at('21:59')).toMatchObject({ color: 'RED', period: 'hp', price: TARIFS.red.hp });
	});

	it('is today’s off-peak price from 22:00 to midnight', () => {
		expect(at('22:00')).toEqual({ color: 'RED', period: 'hc', price: TARIFS.red.hc, until: '06:00', other: { period: 'hp', price: TARIFS.red.hp } });
		expect(at('23:59')).toMatchObject({ color: 'RED', period: 'hc' });
	});

	it('is still yesterday’s colour until 05:59: a Tempo day runs 06:00 to 06:00', () => {
		expect(at('00:00')).toMatchObject({ color: 'WHITE', period: 'hc', price: TARIFS.white.hc, until: '06:00' });
		expect(at('05:59')).toMatchObject({ color: 'WHITE', period: 'hc', price: TARIFS.white.hc });
		expect(at('06:00')).toMatchObject({ color: 'RED', period: 'hp' });
	});

	it('says nothing without prices or without the colour in force', () => {
		expect(priceNow(new Date('2026-12-10T12:00:00'), tempoToday({ tarifs: null }))).toBeNull();
		expect(priceNow(new Date('2026-12-10T03:00:00'), tempoToday({ yesterday: { date: '2026-12-09', color: null } }))).toBeNull();
	});

	it('follows the server’s hours', () => {
		const data = tempoToday({ hours: { peak_start: '07:00', peak_end: '23:00' } });
		expect(priceNow(new Date('2026-12-10T06:30:00'), data)).toMatchObject({ color: 'WHITE', period: 'hc' });
		expect(priceNow(new Date('2026-12-10T22:30:00'), data)).toMatchObject({ color: 'RED', period: 'hp', until: '23:00' });
	});
});
