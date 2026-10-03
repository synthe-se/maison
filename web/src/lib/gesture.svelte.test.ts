import { describe, expect, it, vi } from 'vitest';
import { ApiError } from './api.ts';
import { Gesture, pending, unavailable } from './gesture.svelte.ts';
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

	it('a second press of a control already in flight does nothing; another key goes', async () => {
		const g = new Gesture();
		const send = vi.fn(() => new Promise<void>(() => {}));
		void g.run(send, undefined, 'a');
		expect(await g.run(send, undefined, 'a')).toBeUndefined();
		expect(send).toHaveBeenCalledOnce();
		void g.run(send, undefined, 'b');
		expect(send).toHaveBeenCalledTimes(2);
	});

	it('a failure about a field: said under it (`error`), not in a toast, the focus back on it', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		const field = document.createElement('input');
		document.body.append(field);
		const g = new Gesture();
		await g.run(() => Promise.reject(new Error('Code refusé')), undefined, 'pair', { field: () => field });
		expect(g.error).toBe('Code refusé');
		expect(fail).not.toHaveBeenCalled();
		expect(document.activeElement).toBe(field);
		// the next try starts clean
		void g.run(() => new Promise(() => {}), undefined, 'again');
		expect(g.error).toBe('');
		field.remove();
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

	it('an inline failure: said where the gesture is (`error`), no toast, the focus left where it is', async () => {
		const fail = vi.spyOn(ui, 'fail');
		const g = new Gesture();
		const button = document.createElement('button');
		document.body.append(button);
		button.focus();
		await g.run(() => Promise.reject(new Error('Clé refusée')), undefined, 'signin', { inline: true });
		expect(g.error).toBe('Clé refusée');
		expect(fail).not.toHaveBeenCalled();
		expect(document.activeElement).toBe(button);
		button.remove();
	});

	it('a refusal the caller answers itself says nothing else; one it does not answer is said', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		const g = new Gesture();
		const refused = (e: unknown) => e instanceof ApiError && e.code === 'person_exists';
		await g.run(() => Promise.reject(new ApiError('', 409, 'person_exists')), undefined, 'invite', { refused });
		expect(fail).not.toHaveBeenCalled();
		expect(g.error).toBe('');
		await g.run(() => Promise.reject(new ApiError('Panne', 500)), undefined, 'invite', { refused });
		expect(fail).toHaveBeenCalledOnce();
	});
});

describe('the attributes of a control', () => {
	it('pending: busy and not operable, or nothing (never undoing another spread)', () => {
		expect(pending(true)).toEqual({ 'aria-disabled': 'true', 'aria-busy': 'true' });
		expect(pending(false)).toEqual({});
	});

	it('unavailable: not operable and described by why, or nothing', () => {
		expect(unavailable('tile-state')).toEqual({ 'aria-disabled': 'true', 'aria-describedby': 'tile-state' });
		expect(unavailable(false)).toEqual({});
		expect(unavailable(undefined)).toEqual({});
	});
});
