import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { Gesture } from '#lib/gesture.svelte.ts';
import { locale } from '#lib/i18n.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { clockOf, feed, reread, served, status } from './tuya.svelte.ts';

describe('tuya', () => {
	afterEach(() => forgetAll());

	it('writes the device’s « 19:30 » in the reader’s format', () => {
		const reader = (h: number, min: number) =>
			new Intl.DateTimeFormat(locale(), { hour: 'numeric', minute: '2-digit' }).format(new Date(2000, 0, 1, h, min));
		expect(clockOf('08:05')).toBe(reader(8, 5));
		expect(clockOf('19:30')).toBe(reader(19, 30));
		expect(clockOf('19:30')).toMatch(/7:30|19:30/);
	});

	it('reads a device’s status and keeps it fresh while shown', async () => {
		const api = stubApi({ '/devices/f1/feeder/status': { success: true, parsed_status: { food_level: 'full' } } });
		let entry!: ReturnType<typeof status<{ food_level: string }>>;
		const destroy = $effect.root(() => {
			entry = status('feeder', 'f1');
		});
		flushSync();
		await expect.poll(() => entry.data).toEqual({ food_level: 'full' });
		expect(api.sent('GET', '/devices/f1/feeder/status')).toHaveLength(1);
		destroy();
	});

	it('rereads the shown values under a prefix after a gesture, then says what happened', async () => {
		const api = stubApi({ '/devices/x1/fountain/status': { success: true, parsed_status: { power: true } } });
		const say = vi.spyOn(ui, 'say');
		let entry!: ReturnType<typeof status>;
		const destroy = $effect.root(() => {
			entry = status('fountain', 'x1');
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
