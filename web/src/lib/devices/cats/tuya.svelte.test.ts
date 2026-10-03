import { describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { Gesture } from '#lib/gesture.svelte.ts';
import { live } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { feed, reread, served } from './tuya.svelte.ts';
import { status } from './data.ts';

describe('tuya', () => {
	it('reads a device’s status and keeps it fresh while shown', async () => {
		const reported = { system: { poweredBy: 'AC Power' } };
		const api = stubApi({ '/devices/f1/feeder/status': { success: true, parsedStatus: reported } });
		let entry!: { readonly data: unknown };
		const destroy = $effect.root(() => {
			entry = live(status('feeder', 'f1'));
		});
		flushSync();
		await expect.poll(() => entry.data).toEqual(reported);
		expect(api.sent('GET', '/devices/f1/feeder/status')).toHaveLength(1);
		destroy();
	});

	it('rereads the shown values under a prefix after a gesture, then says what happened', async () => {
		const api = stubApi({ '/devices/x1/fountain/status': { success: true, parsedStatus: { power: true } } });
		const say = vi.spyOn(ui, 'say');
		let entry!: { readonly data: unknown };
		const destroy = $effect.root(() => {
			entry = live(status('fountain', 'x1'));
		});
		flushSync();
		await expect.poll(() => entry.data).toBeDefined();
		await reread('tuya:x1', 'Fontaine allumée')();
		expect(api.sent('GET', '/devices/x1/fountain/status')).toHaveLength(2);
		expect(say).toHaveBeenCalledWith('Fontaine allumée');
		say.mockClear();
		await reread('tuya:x1')();
		expect(say).not.toHaveBeenCalled();
		destroy();
	});

	it('serves portions, remembers when, and says how many', async () => {
		const api = stubApi({ 'POST /devices/f2/feeder/feed': { success: true } });
		const say = vi.spyOn(ui, 'say');
		const g = new Gesture();
		const run = feed(g, 'f2', 3, 'f2');
		expect(g.is('f2')).toBe(true);
		await run;
		expect(api.sent('POST', '/devices/f2/feeder/feed')[0].body).toEqual({ portion: 3 });
		expect(served.f2).toBeGreaterThan(0);
		expect(say).toHaveBeenCalledWith(m.feeder_portions_distributed({ count: 3 }));
		expect(g.is()).toBe(false);
	});

	it('remembers nothing when the feeder refuses', async () => {
		stubApi({ 'POST /devices/f3/feeder/feed': new Response(JSON.stringify({ error: 'offline' }), { status: 503 }) });
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		await feed(new Gesture(), 'f3', 1);
		expect(served.f3).toBeUndefined();
		expect(fail).toHaveBeenCalledOnce();
	});
});
