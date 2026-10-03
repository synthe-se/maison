import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { forgetAll, live, refresh } from './live.svelte.ts';

/** Runs `live()` the way a component would, and returns the value plus the unmount. */
function mount<T>(key: string, fetch: () => Promise<T>, every?: number | ((d: T | undefined) => number)) {
	let entry!: ReturnType<typeof live<T>>;
	const destroy = $effect.root(() => {
		entry = live(key, fetch, every);
	});
	flushSync();
	return { entry, destroy };
}

const settle = () => vi.advanceTimersByTimeAsync(0);

describe('live', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => {
		vi.useRealTimers();
		forgetAll();
	});

	it('loads on mount and exposes the data', async () => {
		const { entry, destroy } = mount('a', async () => 42);
		expect(entry.loading).toBe(true);
		await settle();
		expect(entry.data).toBe(42);
		expect(entry.loading).toBe(false);
		expect(entry.at).toBeGreaterThan(0);
		destroy();
	});

	it('shares one value, and one request, between two views of the same key', async () => {
		const fetch = vi.fn(async () => 'tv');
		const one = mount('tv', fetch);
		const two = mount('tv', fetch);
		await settle();
		expect(one.entry).toBe(two.entry);
		expect(fetch).toHaveBeenCalledOnce();
		one.destroy();
		two.destroy();
	});

	it('polls at its interval while mounted, and stops when the last view goes', async () => {
		const fetch = vi.fn(async () => 1);
		const { destroy } = mount('p', fetch, 1000);
		await settle();
		await vi.advanceTimersByTimeAsync(3000);
		expect(fetch).toHaveBeenCalledTimes(4);
		destroy();
		await vi.advanceTimersByTimeAsync(5000);
		expect(fetch).toHaveBeenCalledTimes(4);
	});

	it('picks its interval from the data (faster while a motor runs)', async () => {
		let moving = true;
		const fetch = vi.fn(async () => ({ moving }));
		const { destroy } = mount('shutters', fetch, (d) => (d?.moving ? 100 : 10_000));
		await settle();
		await vi.advanceTimersByTimeAsync(300);
		const fast = fetch.mock.calls.length;
		expect(fast).toBeGreaterThanOrEqual(3);
		moving = false;
		await vi.advanceTimersByTimeAsync(5000);
		expect(fetch.mock.calls.length).toBeLessThanOrEqual(fast + 1);
		destroy();
	});

	it('keeps the last value when a refresh fails, and says why', async () => {
		let fail = false;
		const { entry, destroy } = mount('f', async () => {
			if (fail) throw new Error('down');
			return 'up';
		});
		await settle();
		fail = true;
		await entry.refresh();
		expect(entry.data).toBe('up');
		expect(entry.error?.message).toBe('down');
		expect(entry.loading).toBe(false);
		destroy();
	});

	it('wraps a thrown non-Error', async () => {
		const { entry, destroy } = mount('n', () => Promise.reject('plain'));
		await settle();
		expect(entry.error).toBeInstanceOf(Error);
		expect(entry.error?.message).toBe('plain');
		destroy();
	});

	it('shows what a command answered at once with set()', async () => {
		const { entry, destroy } = mount('s', async () => 1);
		await settle();
		entry.set(2);
		expect(entry.data).toBe(2);
		destroy();
	});

	it('set() plans the next poll from what it sets (a pairing window that opens counts down)', async () => {
		const fetch = vi.fn(async () => ({ open: false }));
		const { entry, destroy } = mount('pairing', fetch, (d) => (d?.open ? 1_000 : 10_000));
		await settle();
		expect(fetch).toHaveBeenCalledOnce();
		entry.set({ open: true });
		await vi.advanceTimersByTimeAsync(1_000);
		expect(fetch).toHaveBeenCalledTimes(2);
		destroy();
	});

	it('set() while a request travels leaves the planning to its answer', async () => {
		let answer!: (v: number) => void;
		const fetch = vi.fn(() => new Promise<number>((r) => (answer = r)));
		const { entry, destroy } = mount('busy', fetch, 1_000);
		entry.set(5);
		await vi.advanceTimersByTimeAsync(5_000);
		expect(fetch).toHaveBeenCalledOnce();
		answer(6);
		await vi.advanceTimersByTimeAsync(1_000);
		expect(fetch).toHaveBeenCalledTimes(2);
		destroy();
	});

	it('nothing known and a failure: `failed`, an error to show, not an empty list', async () => {
		const { entry, destroy } = mount('down', () => Promise.reject(new Error('down')));
		await settle();
		expect(entry.failed).toBe(true);
		expect(entry.loading).toBe(false);
		destroy();
	});

	it('refresh(prefix) asks again every mounted value under the prefix only', async () => {
		const lamp = vi.fn(async () => 'lamp');
		const plug = vi.fn(async () => 'plug');
		const a = mount('lamps:1', lamp);
		const b = mount('plugs', plug);
		await settle();
		await refresh('lamps');
		expect(lamp).toHaveBeenCalledTimes(2);
		expect(plug).toHaveBeenCalledOnce();
		a.destroy();
		b.destroy();
	});
});
