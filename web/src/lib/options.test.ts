import { describe, expect, it } from 'vitest';
import { options } from './options.ts';

describe('options', () => {
	it('keeps the values’ order, each with its words, from a map of messages or a function', () => {
		const LABEL = { a: () => 'Un', b: () => 'Deux' };
		expect(options(['b', 'a'] as const, LABEL)).toEqual([
			{ value: 'b', label: 'Deux' },
			{ value: 'a', label: 'Un' }
		]);
		expect(options(['x'], (v) => v.toUpperCase())).toEqual([{ value: 'x', label: 'X' }]);
	});

	it('reads the words each time it is called (a change of language reaches them)', () => {
		let word = 'Froid';
		const LABEL = { cool: () => word };
		expect(options(['cool'] as const, LABEL)[0].label).toBe('Froid');
		word = 'Cool';
		expect(options(['cool'] as const, LABEL)[0].label).toBe('Cool');
	});
});
