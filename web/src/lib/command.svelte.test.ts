import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Command } from './command.svelte.ts';
import { ui } from './ui.svelte.ts';

/** A send that resolves (or rejects) when told to. */
function deferred() {
	let resolve!: () => void;
	let reject!: (e: unknown) => void;
	const promise = new Promise<void>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject, send: () => promise };
}

describe('Command', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('shows the target at once, the read state otherwise', async () => {
		const c = new Command(() => 'Lampe', 3000);
		const d = deferred();
		expect(c.shown(false)).toBe(false);
		const run = c.run(true, d.send);
		expect(c.shown(false)).toBe(true);
		expect(c.slow).toBe(false);
		d.resolve();
		expect(await run).toBe(true);
		expect(c.target).toBeUndefined();
		expect(c.shown(false)).toBe(false);
	});

	it('leaves no timer behind once answered (the limit is cleared, not left to fire)', async () => {
		const c = new Command(() => 'Lampe', 30_000);
		const d = deferred();
		const run = c.run(true, d.send);
		d.resolve();
		await run;
		expect(vi.getTimerCount()).toBe(0);
	});

	it('says it is working after 1 s, not before', async () => {
		const c = new Command(() => 'Lampe', 3000);
		const d = deferred();
		const run = c.run(true, d.send);
		await vi.advanceTimersByTimeAsync(999);
		expect(c.slow).toBe(false);
		await vi.advanceTimersByTimeAsync(1);
		expect(c.slow).toBe(true);
		d.resolve();
		await run;
		expect(c.slow).toBe(false);
	});

	it('gives up at its limit: back to the read state, « no answer » on the tile and for readers', async () => {
		const say = vi.spyOn(ui, 'say');
		const c = new Command(() => 'Lampe du salon', 3000);
		const run = c.run(true, deferred().send);
		await vi.advanceTimersByTimeAsync(3000);
		expect(await run).toBe(false);
		expect(c.late).toBe(true);
		expect(c.target).toBeUndefined();
		expect(say).toHaveBeenCalledWith(expect.stringContaining('Lampe du salon'));
	});

	it('treats an error like no answer', async () => {
		const c = new Command(() => 'Prise', 5000);
		vi.spyOn(console, 'warn').mockImplementation(() => {});
		const ok = await c.run(false, () => Promise.reject(new Error('boom')));
		expect(ok).toBe(false);
		expect(c.late).toBe(true);
	});

	it('a newer gesture takes over: the older answer changes nothing', async () => {
		const c = new Command(() => 'Lampe', 3000);
		const first = deferred();
		const second = deferred();
		const one = c.run(true, first.send);
		const two = c.run(false, second.send);
		first.resolve();
		expect(await one).toBe(false);
		expect(c.target).toBe(false);
		second.resolve();
		expect(await two).toBe(true);
		expect(c.target).toBeUndefined();
	});

	it('a new try clears the previous « no answer »', async () => {
		const c = new Command(() => 'Lampe', 100);
		const late = c.run(true, deferred().send);
		await vi.advanceTimersByTimeAsync(100);
		await late;
		expect(c.late).toBe(true);
		const retry = c.run(true, () => Promise.resolve());
		expect(c.late).toBe(false);
		await retry;
	});
});
