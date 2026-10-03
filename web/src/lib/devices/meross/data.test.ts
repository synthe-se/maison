import { describe, expect, it } from 'vitest';
import {
	CONSUMPTION_EVERY_MS,
	ELECTRICITY_EVERY_MS,
	LIST_EVERY_MS,
	MEROSS,
	STATUS_EVERY_MS,
	consumption,
	electricity,
	plugs,
	status
} from './data.ts';

describe('plug sources', () => {
	it('every key starts with « meross », so one refresh after a gesture asks them all again', () => {
		for (const s of [plugs, status('a'), electricity('a'), consumption('a')]) expect(s.key.startsWith(MEROSS)).toBe(true);
	});

	it('one key per plug and per reading', () => {
		expect(new Set([status('a'), status('b'), electricity('a'), consumption('a')].map((s) => s.key)).size).toBe(4);
	});

	it('one fetch per key: the tile and the page ask a plug the same way', () => {
		expect(electricity('a')).toBe(electricity('a'));
		expect(electricity('a').fetch).toBe(electricity('a').fetch);
	});

	it('keeps the old cadences: 5 s list and power, 3 s status, 30 s daily totals', () => {
		expect([LIST_EVERY_MS, ELECTRICITY_EVERY_MS, STATUS_EVERY_MS, CONSUMPTION_EVERY_MS]).toEqual([5000, 5000, 3000, 30000]);
	});
});
