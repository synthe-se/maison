import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { REMOTE_ROWS, keyName } from './keys.ts';

describe('remote keys', () => {
	it('names a key by its name, or by what is printed on it', () => {
		expect(keyName(116)).toBe(m.key_power());
		expect(keyName(5)).toBe('4');
		expect(keyName(353)).toBe('OK');
		expect(keyName(365)).toBe(m.key_guide());
	});

	it('says « n° 412 » for a code not on the picture', () => {
		expect(keyName(412)).toMatch(/^n°\s412$/u);
	});

	it('has each code once, each key either an icon or a glyph', () => {
		const keys = REMOTE_ROWS.flat();
		expect(new Set(keys.map((k) => k.code)).size).toBe(keys.length);
		for (const k of keys) expect(Boolean(k.icon) !== Boolean(k.glyph)).toBe(true);
	});
});
