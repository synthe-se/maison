import { describe, expect, it, vi } from 'vitest';
import { Gesture } from '#lib/gesture.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { keySender } from './remote.ts';

describe('keySender', () => {
	it('each press its own key: presses never wait for one another, nothing shows in flight per press', async () => {
		const g = new Gesture();
		const send = vi.fn((k: string) => new Promise<string>((r) => setTimeout(() => r(k), 10)));
		const press = keySender(g, send);
		const both = Promise.all([press('up'), press('up')]);
		expect(send).toHaveBeenCalledTimes(2);
		await both;
		expect(g.is()).toBe(false);
	});

	it('a failure is said once, by the gesture', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		const press = keySender(new Gesture(), () => Promise.reject(new Error('TV unreachable')));
		await press('ok');
		expect(fail).toHaveBeenCalledOnce();
	});
});
