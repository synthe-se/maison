import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { facts, longDate, when } from '#lib/i18n.svelte.ts';
import { lifeLine } from './lines.ts';

const words = { since: m.keys_created, used: m.keys_used, never: m.keys_never_used };

describe('lifeLine', () => {
	it('since when, then when last used', () => {
		const created = new Date(2026, 10, 12).getTime();
		const used = new Date(2026, 9, 3, 21, 4).getTime();
		expect(lifeLine(created, used, words)).toBe(facts(m.keys_created({ when: longDate(created) }), m.keys_used({ when: when(used) })));
	});

	it('or never used', () => {
		const created = new Date(2026, 10, 12).getTime();
		expect(lifeLine(created, null, words)).toBe(facts(m.keys_created({ when: longDate(created) }), m.keys_never_used()));
	});
});
