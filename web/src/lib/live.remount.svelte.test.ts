import { describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';
import { forgetAll, live, source } from './live.svelte.ts';

describe('a live value outlives the view that first asked for it', () => {
	it('its deriveds stay live after that view is destroyed (« back » to a page)', async () => {
		const src = source('remount-test', async () => 1);
		let first!: ReturnType<typeof live<number>>;
		const destroyFirst = $effect.root(() => void (first = live(src)));
		flushSync();
		await first.refresh();
		destroyFirst();

		// the next view reads the same entry: loading follows the data, never a stale answer
		let second!: ReturnType<typeof live<number>>;
		const destroySecond = $effect.root(() => void (second = live(src)));
		flushSync();
		expect(second).toBe(first);
		expect(second.loading).toBe(false);
		second.set(2);
		flushSync();
		expect(second.data).toBe(2);
		expect(second.loading).toBe(false);
		destroySecond();
		forgetAll();
	});
});
