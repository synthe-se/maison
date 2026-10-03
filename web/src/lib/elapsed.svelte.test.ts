import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { Elapsed } from './elapsed.svelte.ts';

describe('Elapsed', () => {
	afterEach(() => vi.useRealTimers());

	it('counts from the moment the step starts, ticking while it runs, back to 0 once it stops', async () => {
		vi.useFakeTimers();
		const step = $state({ running: false });
		let e!: Elapsed;
		const destroy = $effect.root(() => {
			e = new Elapsed(() => step.running, 1_000);
		});
		flushSync();
		expect(e.seconds).toBe(0);
		await vi.advanceTimersByTimeAsync(5_000);
		step.running = true;
		flushSync();
		expect(e.seconds).toBe(0);
		await vi.advanceTimersByTimeAsync(3_000);
		expect(e.seconds).toBe(3);
		expect(e.ms).toBe(3_000);
		step.running = false;
		flushSync();
		expect(e.seconds).toBe(0);
		destroy();
	});
});
