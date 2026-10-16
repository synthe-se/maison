import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { facts, longDate, when } from '#lib/i18n.svelte.ts';
import { browserName, lifeLine } from './lines.ts';

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

describe('browserName', () => {
	it.each([
		[
			'Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Mobile/15E148 Safari/604.1',
			'Safari'
		],
		['Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36', 'Chrome'],
		['Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36 Edg/140.0', 'Edge'],
		['Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0', 'Firefox'],
		['Mozilla/5.0 (iPhone) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/140.0 Mobile/15E148 Safari/604.1', 'Chrome'],
		['curl/8.7.1', '']
	])('%s → %s', (ua, name) => expect(browserName(ua)).toBe(name));
});
