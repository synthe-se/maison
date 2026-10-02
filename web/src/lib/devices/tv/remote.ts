// What both remotes (the Philips TV and the Android TV box) share: the poll interval, the
// repeat-on-hold rule and the keys a pad knows.

import { m } from '#lib/paraglide/messages.js';
import { CONFIRM, TAP } from '#lib/haptics.ts';
import type { IconName } from '#lib/components/Icon.svelte';

/**
 * Both are read once a minute (docs/ux/tableau-de-bord.md § 4). Deliberately slow for the TV:
 * its JointSPACE HTTP server is single-threaded and dies for good under bursts (only a mains
 * power cycle revives it), so the dashboard asks rarely and the backend spaces every call it
 * makes (900 ms between two, a status read is four of them). Never poll it faster.
 */
export const REMOTE_POLL = 60_000;

/**
 * Repeat on hold (docs/ux/tableau-de-bord.md § 7, NSStepper.autorepeat): one step on press,
 * then after 500 ms ten steps a second. Volume is capped at five a second, what the TV follows.
 */
export const REPEAT_DELAY = 500;
export const REPEAT_EVERY = 100;
export const REPEAT_EVERY_VOLUME = 200;
/** A press stays visible at least this long, the only feedback on an iPhone (no web vibration). */
export const PRESSED_MIN = 100;

/** The keys a pad sends, named once; each remote maps them to its own API's keys. */
export type PadKey = 'up' | 'down' | 'left' | 'right' | 'ok' | 'back' | 'home' | 'menu' | 'volume_up' | 'volume_down' | 'mute';

/** How often a key repeats while held, or 0: never for OK, Back, Home, mute (§ 7). */
export function repeatEvery(key: PadKey): number {
	if (key === 'volume_up' || key === 'volume_down') return REPEAT_EVERY_VOLUME;
	if (key === 'up' || key === 'down' || key === 'left' || key === 'right') return REPEAT_EVERY;
	return 0;
}

// Repeat-on-hold, for the remotes' keys only (nothing else repeats): `paced` and `holdRepeat`
// are the whole rule.

/**
 * A sender whose repeats wait for the device: a first press always goes, a repeat is dropped
 * while an earlier request is still in flight. The TV's backend gate spaces calls by 900 ms;
 * without this, ten repeats a second would queue up and the cursor would keep moving for
 * seconds after the finger let go.
 */
export function paced(send: () => Promise<unknown>) {
	let inflight = 0;
	return async (repeat: boolean) => {
		if (repeat && inflight > 0) return;
		inflight++;
		try {
			await send();
		} finally {
			inflight--;
		}
	};
}

/**
 * Starts repeating `fire(true)` after REPEAT_DELAY, every `every` ms (the first press is the
 * caller's). Returns the function that stops it.
 */
export function holdRepeat(fire: (repeat: boolean) => unknown, every: number): () => void {
	if (every <= 0) return () => {};
	let timer = setTimeout(function tick() {
		void fire(true);
		timer = setTimeout(tick, every);
	}, REPEAT_DELAY);
	return () => clearTimeout(timer);
}

/** Each pad key's name (in words, never the code: § 7), icon and touch feedback, once. */
export const PAD_KEY: Record<PadKey, { label: () => string; icon?: IconName; pattern: typeof TAP }> = {
	up: { label: m.remote_keys_up, icon: 'chevron-up', pattern: TAP },
	down: { label: m.remote_keys_down, icon: 'chevron-down', pattern: TAP },
	left: { label: m.remote_keys_left, icon: 'chevron-left', pattern: TAP },
	right: { label: m.remote_keys_right, icon: 'chevron-right', pattern: TAP },
	ok: { label: m.tv_key_ok, icon: 'circle', pattern: CONFIRM },
	back: { label: m.tv_key_back, icon: 'corner-up-left', pattern: TAP },
	home: { label: m.remote_keys_home, icon: 'house', pattern: TAP },
	menu: { label: m.remote_keys_menu, icon: 'menu', pattern: TAP },
	volume_up: { label: m.tv_volume_up, icon: 'plus', pattern: TAP },
	volume_down: { label: m.tv_volume_down, icon: 'minus', pattern: TAP },
	mute: { label: m.tv_mute, icon: 'volume-x', pattern: CONFIRM }
};

export type Fire = (repeat: boolean) => Promise<void>;

/**
 * One paced sender per key, shared by the pad's buttons, its keyboard shortcuts and any key
 * shown elsewhere on the tile (the TV's volume − / +), so a hold paces them all alike.
 */
export function pacedKeys(send: (key: PadKey) => Promise<unknown>): Record<PadKey, Fire> {
	const keys = Object.keys(PAD_KEY) as PadKey[];
	return Object.fromEntries(keys.map((k) => [k, paced(() => send(k))])) as Record<PadKey, Fire>;
}
