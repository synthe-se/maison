import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { time } from './clock.svelte.ts';
import { live, refresh, source, sources, STALE_CADENCES, type Every, type Source } from './live.svelte.ts';

/** Runs `live()` the way a component would, and returns the value plus the unmount. */
function mountSource<T, A = never>(src: Source<T, A>) {
	let entry!: ReturnType<typeof live<T, A>>;
	const destroy = $effect.root(() => {
		entry = live(src);
	});
	flushSync();
	return { entry, destroy };
}
const mount = <T>(key: string, fetch: () => Promise<T>, every?: Every<T>) => mountSource(source(key, fetch, every));

const settle = () => vi.advanceTimersByTimeAsync(0);

describe('live', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => {
		vi.useRealTimers();
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

	it('one fetch per key: declaring a key again with another fetch is refused (in development)', () => {
		const a = mount('dup', async () => 1);
		expect(() => mount('dup', async () => 2)).toThrow(/dup/);
		a.destroy();
	});

	it('a family of sources gives back one source per key, so every view asks the same way', () => {
		const plug = sources((id: string) => source(`plug:${id}`, async () => id));
		expect(plug('a')).toBe(plug('a'));
		expect(plug('a')).not.toBe(plug('b'));
	});

	it('says a value is old: its last ask failed, or no answer for STALE_CADENCES polls', async () => {
		let fail = false;
		const { entry, destroy } = mount(
			'old',
			async () => {
				if (fail) throw new Error('down');
				return 1;
			},
			1_000
		);
		await settle();
		expect(entry.stale).toBe(false);
		fail = true;
		await vi.advanceTimersByTimeAsync(1_000);
		expect(entry.stale).toBe(true);
		expect(entry.data).toBe(1);
		fail = false;
		await vi.advanceTimersByTimeAsync(1_000);
		expect(entry.stale).toBe(false);
		// the polls stopped (a hidden tab, a frozen timer): old once the clock is past them
		time.now = new Date(entry.at + (STALE_CADENCES + 1) * 1_000);
		expect(entry.stale).toBe(true);
		time.now = new Date();
		destroy();
	});

	it('update() changes part of what is known; nothing known, nothing to change', async () => {
		const { entry, destroy } = mount('u', async () => ({ a: 1, b: 2 }));
		entry.update((d) => ({ ...d, b: 3 }));
		expect(entry.data).toBeUndefined();
		await settle();
		entry.update((d) => ({ ...d, b: 3 }));
		expect(entry.data).toEqual({ a: 1, b: 3 });
		destroy();
	});

	it('refresh(arg) is an ask of its own (a forced scan): its argument reaches the fetch, and `since` starts again', async () => {
		const fetch = vi.fn(async (force?: boolean) => (force ? 'scan' : 'cache'));
		const { entry, destroy } = mountSource(source('scan', fetch));
		await settle();
		const started = entry.since;
		await vi.advanceTimersByTimeAsync(10);
		await entry.refresh(true);
		expect(fetch).toHaveBeenLastCalledWith(true);
		expect(entry.data).toBe('scan');
		expect(entry.since).toBeGreaterThan(started);
		destroy();
	});

	it('the cadence may stop once a search has gone on long enough (`since`)', async () => {
		const fetch = vi.fn(async () => 0);
		const { destroy } = mount('search', fetch, (_d, since) => (Date.now() - since < 3_000 ? 1_000 : 0));
		await settle();
		await vi.advanceTimersByTimeAsync(10_000);
		expect(fetch.mock.calls.length).toBeLessThanOrEqual(4);
		destroy();
	});

	it('ready: the first answer, once (now when one is known)', async () => {
		const { entry, destroy } = mount('r', async () => 'first');
		await expect(entry.ready).resolves.toBe('first');
		destroy();
	});
});
