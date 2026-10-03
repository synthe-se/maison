import { afterEach, describe, expect, it, vi } from 'vitest';
import { json, stored, text } from './stored.ts';

const KEY = 'maison-test-stored';

describe('stored', () => {
	afterEach(() => localStorage.removeItem(KEY));

	it('reads what was kept, or the fallback; `undefined` forgets it', () => {
		const order = stored<{ on: boolean } | null>(
			KEY,
			null,
			json((v): v is { on: boolean } => typeof v === 'object' && v !== null && 'on' in v)
		);
		expect(order.get()).toBeNull();
		order.set({ on: true });
		expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ on: true });
		expect(order.get()).toEqual({ on: true });
		order.set(undefined);
		expect(localStorage.getItem(KEY)).toBeNull();
	});

	it('a word kept as itself (what app.html reads before any script); one it does not know is the fallback', () => {
		const theme = stored(KEY, 'system', text(['light', 'dark', 'system']));
		theme.set('dark');
		expect(localStorage.getItem(KEY)).toBe('dark');
		localStorage.setItem(KEY, 'sepia');
		expect(theme.get()).toBe('system');
	});

	it('never fails: unreadable JSON or a refused storage gives the fallback, a write is lost quietly', () => {
		const n = stored(
			KEY,
			0,
			json((v): v is number => typeof v === 'number')
		);
		localStorage.setItem(KEY, '{oops');
		expect(n.get()).toBe(0);
		vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('private mode');
		});
		expect(() => n.set(3)).not.toThrow();
	});
});
