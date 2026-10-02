import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { kwh, kwhFromWh, livePower, milliamps, reading, volts, watts } from './units.ts';
import { merossElectricity } from '#lib/test/meross.ts';

// French numbers: decimal comma; Intl's no-break spaces are kept as they come
const fr = (n: number, digits: number) =>
	new Intl.NumberFormat('fr', { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(n);

describe('electricity in words', () => {
	it('says watts with no decimal on a tile, one on the plug’s page', () => {
		expect(watts(42.46)).toBe(m.meross_watts({ power: '42' }));
		expect(watts(42.46, 1)).toBe(m.meross_watts({ power: '42,5' }));
	});

	it('says volts with one decimal', () => {
		expect(volts(230.04)).toBe(m.meross_volts({ voltage: '230,0' }));
	});

	it('says a small current in milliamperes', () => {
		expect(milliamps(0.183)).toBe(m.meross_milliamps({ current: '183' }));
	});

	it('says energy in kWh with three decimals, from kWh or from Wh', () => {
		expect(kwh(1.5)).toBe(m.meross_kwh({ energy: fr(1.5, 3) }));
		expect(kwhFromWh(1234)).toBe(m.meross_kwh({ energy: fr(1.234, 3) }));
		expect(kwhFromWh(1234)).toMatch(/^1,234\skWh$/);
	});

	it('converts the raw reading: tenths of a volt, milliamperes, milliwatts', () => {
		expect(reading({ voltage: 2301, current: 183, power: 42400 })).toEqual({ volts: 230.1, amps: 0.183, watts: 42.4 });
	});

	it('reads the live power from the raw milliwatts, not the « 42.4W » string', () => {
		expect(livePower(merossElectricity().electricity)).toBe(m.meross_watts({ power: '42' }));
	});
});
