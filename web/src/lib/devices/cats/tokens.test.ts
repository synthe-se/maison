import { afterEach, describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { LITTER_STATE, STANDBY, WATER_LEVEL, litterHalf, waterLow, wordOf } from './tokens.ts';

afterEach(() => vi.restoreAllMocks());

describe('the cat devices’ tokens', () => {
	it('words a known token', () => {
		expect(wordOf(LITTER_STATE, STANDBY)).toBe(m.state_standby());
		expect(wordOf(WATER_LEVEL, 'ok')).toBe(m.fountain_water_level_ok());
	});

	it('an unknown token, or none, is « Inconnu »: the unknown one logged once', () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		expect(wordOf(LITTER_STATE, 'dumping')).toBe(m.common_unknown());
		expect(wordOf(LITTER_STATE, 'dumping')).toBe(m.common_unknown());
		expect(wordOf(LITTER_STATE, undefined)).toBe(m.common_unknown());
		expect(wordOf(LITTER_STATE, 'toString')).toBe(m.common_unknown());
		expect(warn).toHaveBeenCalledTimes(2);
	});

	it('says what needs a hand', () => {
		expect(litterHalf({ sensors: { litterLevel: 'half' } })).toBe(true);
		expect(litterHalf({ sensors: { litterLevel: 'full' } })).toBe(false);
		expect(waterLow({ waterLevel: 'low' })).toBe(true);
		expect(waterLow(undefined)).toBe(false);
	});
});
