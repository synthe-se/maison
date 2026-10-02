import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import {
	PAD_KEY,
	REMOTE_POLL,
	REPEAT_DELAY,
	REPEAT_EVERY,
	REPEAT_EVERY_VOLUME,
	holdRepeat,
	paced,
	pacedKeys,
	repeatEvery,
	type PadKey
} from './remote.ts';

/** A send that resolves when told to. */
function deferred() {
	const pending: (() => void)[] = [];
	const send = vi.fn(() => new Promise<void>((r) => pending.push(r)));
	return { send, answer: () => pending.shift()?.() };
}

describe('remote timing', () => {
	it('reads the remotes once a minute, never faster (the TV dies under bursts)', () => {
		expect(REMOTE_POLL).toBe(60_000);
	});

	it('repeats the arrows ten times a second, the volume five, never OK / Back / Home / mute', () => {
		expect([REPEAT_DELAY, REPEAT_EVERY, REPEAT_EVERY_VOLUME]).toEqual([500, 100, 200]);
		for (const k of ['up', 'down', 'left', 'right'] as const) expect(repeatEvery(k)).toBe(100);
		expect(repeatEvery('volume_up')).toBe(200);
		expect(repeatEvery('volume_down')).toBe(200);
		for (const k of ['ok', 'back', 'home', 'menu', 'mute'] as const) expect(repeatEvery(k)).toBe(0);
	});

	it('names every key in words', () => {
		expect(PAD_KEY.up.label()).toBe(m.remote_keys_up());
		expect(PAD_KEY.ok.label()).toBe(m.tv_key_ok());
		expect(PAD_KEY.mute.label()).toBe(m.tv_mute());
		for (const k of Object.keys(PAD_KEY) as PadKey[]) expect(PAD_KEY[k].label()).not.toBe('');
	});
});

describe('paced', () => {
	it('a first press always goes, even while an earlier one travels', async () => {
		const d = deferred();
		const fire = paced(d.send);
		void fire(false);
		void fire(false);
		expect(d.send).toHaveBeenCalledTimes(2);
		d.answer();
		d.answer();
	});

	it('drops a repeat while a request is in flight, sends the next once it is answered', async () => {
		const d = deferred();
		const fire = paced(d.send);
		const first = fire(false);
		await fire(true);
		await fire(true);
		expect(d.send).toHaveBeenCalledTimes(1);
		d.answer();
		await first;
		const again = fire(true);
		expect(d.send).toHaveBeenCalledTimes(2);
		d.answer();
		await again;
	});

	it('a failed request frees the way for the next repeat', async () => {
		const send = vi.fn().mockRejectedValueOnce(new Error('TV unreachable')).mockResolvedValue(undefined);
		const fire = paced(send);
		await expect(fire(false)).rejects.toThrow('TV unreachable');
		await fire(true);
		expect(send).toHaveBeenCalledTimes(2);
	});
});

describe('holdRepeat', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('waits 500 ms, then repeats every 100 ms until stopped', () => {
		const fire = vi.fn();
		const stop = holdRepeat(fire, REPEAT_EVERY);
		vi.advanceTimersByTime(499);
		expect(fire).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(fire).toHaveBeenCalledExactlyOnceWith(true);
		vi.advanceTimersByTime(500);
		expect(fire).toHaveBeenCalledTimes(6);
		stop();
		vi.advanceTimersByTime(1000);
		expect(fire).toHaveBeenCalledTimes(6);
	});

	it('repeats the volume every 200 ms', () => {
		const fire = vi.fn();
		const stop = holdRepeat(fire, REPEAT_EVERY_VOLUME);
		vi.advanceTimersByTime(500 + 1000);
		expect(fire).toHaveBeenCalledTimes(6);
		stop();
	});

	it('never repeats a key that does not repeat; stopping before the delay sends nothing', () => {
		const fire = vi.fn();
		holdRepeat(fire, 0)();
		holdRepeat(fire, REPEAT_EVERY)();
		vi.advanceTimersByTime(5000);
		expect(fire).not.toHaveBeenCalled();
	});
});

describe('pacedKeys', () => {
	it('gives every pad key its own paced sender', async () => {
		const sent: PadKey[] = [];
		const keys = pacedKeys(async (k) => void sent.push(k));
		expect(Object.keys(keys).sort()).toEqual(Object.keys(PAD_KEY).sort());
		await keys.up(false);
		await keys.mute(false);
		expect(sent).toEqual(['up', 'mute']);
	});

	it('a held key in flight does not hold back another key', async () => {
		const d = deferred();
		const keys = pacedKeys(d.send);
		void keys.up(false);
		void keys.down(true);
		expect(d.send).toHaveBeenCalledTimes(2);
		d.answer();
		d.answer();
	});
});
