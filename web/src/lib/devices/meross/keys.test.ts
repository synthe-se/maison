import { describe, expect, it } from 'vitest';
import { CONSUMPTION_EVERY_MS, ELECTRICITY_EVERY_MS, LIST_EVERY_MS, LIST_KEY, STATUS_EVERY_MS, consumptionKey, electricityKey, statusKey } from './keys.ts';

describe('plug keys', () => {
	it('every key starts with « meross », so one refresh after a gesture asks them all again', () => {
		for (const k of [LIST_KEY, statusKey('a'), electricityKey('a'), consumptionKey('a')]) expect(k.startsWith('meross')).toBe(true);
	});

	it('one key per plug and per reading', () => {
		expect(new Set([statusKey('a'), statusKey('b'), electricityKey('a'), consumptionKey('a')]).size).toBe(4);
	});

	it('keeps the old cadences: 5 s list and power, 3 s status, 30 s daily totals', () => {
		expect([LIST_EVERY_MS, ELECTRICITY_EVERY_MS, STATUS_EVERY_MS, CONSUMPTION_EVERY_MS]).toEqual([5000, 5000, 3000, 30000]);
	});
});
