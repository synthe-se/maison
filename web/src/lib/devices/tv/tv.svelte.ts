// The Philips TV, driven the same way from its dashboard tile and its page: power over
// infrared (the only channel that wakes the set from deep standby), the state read from
// JointSPACE, the volume stepped from the last value asked. When the set is up but JointSPACE
// is silent (its server crashed: only a mains power cycle revives it), the state is assumed,
// not read: two buttons and the last order instead of a pressed icon (docs/ux.md § 2).
// Construct during component initialisation (it reads the TV through live()).

import { m } from '#lib/paraglide/messages.js';
import { live } from '#lib/live.svelte.ts';
import { Command, LIMIT } from '#lib/command.svelte.ts';
import { Gesture } from '#lib/gesture.svelte.ts';
import { Elapsed } from '#lib/elapsed.svelte.ts';
import { stored, json } from '#lib/stored.ts';
import { CONFIRM, haptic } from '#lib/haptics.ts';
import { PUBLIC_TV_ORDER_KEY } from '$app/env/public';
import { tvApi, type TvKey, type TvStatus } from './api.ts';
import { tv as tvData } from './data.ts';
import { keySender, pacedKeys, type PadKey } from './remote.ts';
import { assumedOn, tvFact, tvState } from './words.ts';

/** The pad's keys in JointSPACE's names; the volume keys go through setVolume instead. */
const TV_KEY: Record<Exclude<PadKey, 'volume_up' | 'volume_down' | 'mute'>, TvKey> = {
	up: 'cursor_up',
	down: 'cursor_down',
	left: 'cursor_left',
	right: 'cursor_right',
	ok: 'confirm',
	back: 'back',
	home: 'home',
	menu: 'options'
};

/** The last power order: what an assumed-state tile says instead of a state (§ 2, case 3). */
type Order = { on: boolean; at: number };
const lastOrder = stored<Order | null>(
	PUBLIC_TV_ORDER_KEY,
	null,
	json((v): v is Order => typeof v === 'object' && v !== null && 'on' in v && 'at' in v)
);

export class TvRemote {
	// Slow on purpose: the set's JointSPACE server is single-threaded and dies for good under
	// bursts, so it is asked once a minute and the backend spaces every call (REMOTE_POLL).
	// Nothing here polls faster; a gesture re-reads once, or takes what its answer says.
	tv = live(tvData);
	status = $derived(this.tv.data?.status);
	config = $derived(this.tv.data?.config);
	/** The set's own name, else one that is not the group's (« TV du salon »). */
	name = $derived(this.status?.name || m.tv_default_name());
	configured = $derived(this.status?.configured ?? false);
	assumed = $derived(assumedOn(this.status));
	volume = $derived(this.status?.power === 'on' ? this.status.volume : undefined);
	/** Line 2 of the tile, in words. */
	state = $derived(tvState(this.status));
	/** The tile's one fact: the volume. */
	fact = $derived(tvFact(this.volume));

	power = new Command(() => this.name, LIMIT.tv);
	/** A button's gesture (assumed power, box, Ambilight). */
	g = new Gesture();
	/** Keys and volume: each its own key, they never wait for one another and show nothing. */
	pad = new Gesture();
	run = keySender(this.pad, (send: () => Promise<unknown>) => send());
	lastOrder = $state(lastOrder.get());

	// waking from deep standby (~20 s): a bar on the elapsed time (§ 2, § 6)
	#fromDeep = $state(false);
	waking = $derived(this.#fromDeep && this.power.target === true);
	#wake = new Elapsed(() => this.waking, 250);

	pendingLevel = $state<number | undefined>(undefined);
	shownLevel = $derived(this.pendingLevel ?? this.volume?.current ?? 0);

	keys = pacedKeys((k) => this.send(k));

	/** Seconds since the wake-up began, at most the TV's limit. */
	get elapsed() {
		return Math.min(LIMIT.tv, this.#wake.ms) / 1000;
	}

	patch(next: Partial<TvStatus>) {
		this.tv.update((d) => ({ ...d, status: { ...d.status, ...next } }));
	}

	async #sendPower(on: boolean) {
		const r = await tvApi.power(on ? 'on' : 'off');
		this.lastOrder = { on, at: Date.now() };
		lastOrder.set(this.lastOrder);
		this.patch({ power: r.power });
		void this.tv.refresh();
	}

	/** Power, read state: the tile's icon, through Command (30 s for the TV). */
	toggle = (on: boolean) => {
		haptic(CONFIRM);
		this.#fromDeep = on && this.status?.power === 'deep_standby';
		void this.power.run(on, () => this.#sendPower(on));
	};

	/** Power, assumed state: two plain buttons. */
	assumedPower = (on: boolean) => {
		haptic(CONFIRM);
		return this.g.run(() => this.#sendPower(on), undefined, on ? 'on' : 'off');
	};

	async setVolume(level?: number, muted?: boolean) {
		const r = await tvApi.setVolume(level, muted);
		this.patch({ volume: r.volume });
	}

	/** A volume step from the last value asked, so quick taps add up. */
	async step(delta: number) {
		const v = this.volume;
		if (!v) return;
		const level = Math.min(v.max, Math.max(v.min, this.shownLevel + delta));
		this.pendingLevel = level;
		await this.run(() => this.setVolume(level));
		if (this.pendingLevel === level) this.pendingLevel = undefined;
	}

	send(k: PadKey): Promise<unknown> {
		if (k === 'volume_up') return this.step(1);
		if (k === 'volume_down') return this.step(-1);
		if (k === 'mute') return this.run(() => this.setVolume(undefined, !this.volume?.muted));
		// no re-read after a key: nothing in the status changes, and a read is four calls
		return this.run(() => tvApi.sendKey(TV_KEY[k]));
	}

	key = (k: TvKey) => () => this.run(() => tvApi.sendKey(k));
}
