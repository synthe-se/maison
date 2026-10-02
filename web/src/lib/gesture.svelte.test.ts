import { describe, expect, it, vi } from 'vitest';
import { Gesture } from './gesture.svelte.ts';
import { ui } from './ui.svelte.ts';

describe('Gesture', () => {
	it('is busy with its key while the command travels, then hands the answer on', async () => {
		const g = new Gesture();
		let resolve!: (v: number) => void;
		const then = vi.fn();
		const run = g.run(() => new Promise<number>((r) => (resolve = r)), then, 'feed');
		expect(g.is('feed')).toBe(true);
		expect(g.is('other')).toBe(false);
		expect(g.is()).toBe(true);
		resolve(3);
		expect(await run).toBe(3);
		expect(then).toHaveBeenCalledWith(3);
		expect(g.is()).toBe(false);
	});

	it('tells a failure once and answers undefined', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		const g = new Gesture();
		const then = vi.fn();
		expect(await g.run(() => Promise.reject(new Error('down')), then)).toBeUndefined();
		expect(fail).toHaveBeenCalledOnce();
		expect(then).not.toHaveBeenCalled();
		expect(g.is()).toBe(false);
	});

	it('a failure in the follow-up (reading the device again) is told too', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		const g = new Gesture();
		await g.run(
			() => Promise.resolve(1),
			() => Promise.reject(new Error('read failed'))
		);
		expect(fail).toHaveBeenCalledOnce();
	});

	it('a newer gesture keeps its busy mark when an older one ends', async () => {
		const g = new Gesture();
		let first!: () => void;
		const one = g.run(() => new Promise<void>((r) => (first = r)), undefined, 'a');
		const two = g.run(() => new Promise<void>(() => {}), undefined, 'b');
		first();
		await one;
		expect(g.is('b')).toBe(true);
		void two;
	});
});
