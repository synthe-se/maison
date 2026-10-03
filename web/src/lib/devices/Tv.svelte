<script lang="ts">
	// The Philips TV: power, volume, Ambilight, a pad and the box's input, driven by JointSPACE.
	// Power goes over infrared (the only channel that wakes the set from deep standby); the state
	// is read from JointSPACE. When the set is up but JointSPACE is silent (its server crashed:
	// only a mains power cycle revives it), the state is assumed, not read, and the tile shows
	// two buttons and the last order instead of a pressed icon (docs/ux/tableau-de-bord.md § 2).
	import { m } from '#lib/paraglide/messages.js';
	import { tvApi, type TvKey, type TvPower, type TvStatus } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { clock } from '#lib/i18n.svelte.ts';
	import { Command, LIMIT } from '#lib/command.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { CONFIRM, haptic, TAP } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Key from './tv/Key.svelte';
	import Pad from './tv/Pad.svelte';
	import More from './tv/More.svelte';
	import Progress from '#lib/components/Progress.svelte';
	import Settings from './tv/Settings.svelte';
	import { pacedKeys, REMOTE_POLL, type PadKey } from './tv/remote.ts';

	// Slow on purpose: the set's JointSPACE server is single-threaded and dies for good under
	// bursts (only a mains cycle revives it), so the dashboard asks once a minute and the
	// backend spaces every call it makes (see REMOTE_POLL). Nothing here polls faster; a gesture
	// re-reads once, or patches what its answer says.
	const tv = live('tv', tvApi.status, REMOTE_POLL);
	const status = $derived(tv.data?.status);
	const config = $derived(tv.data?.config);
	// the set's own name, else one that is not the group's (« Télévision » > « TV du salon »)
	const name = $derived(status?.name || m.tv_default_name());
	const configured = $derived(status?.configured ?? false);
	/** Powered up, JointSPACE silent: the backend reports « on » but reads nothing else. */
	const assumed = $derived(status?.power === 'on' && !status.volume);
	const volume = $derived(status?.power === 'on' ? status.volume : undefined);

	const POWER_STATE: Record<TvPower, () => string> = {
		on: m.state_on,
		standby: m.state_standby,
		deep_standby: m.tv_deep_standby
	};
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

	const power = new Command(() => name, LIMIT.tv);
	let settingsOpen = $state(false);
	let settingsButton = $state<HTMLButtonElement>();
	// two gestures: keys and volume fly while a button's gesture travels (box, Ambilight, power
	// when assumed), and must not clear that button's mark; they show nothing in flight
	const g = new Gesture();
	const pad = new Gesture();

	function patch(next: Partial<TvStatus>) {
		if (tv.data) tv.set({ ...tv.data, status: { ...tv.data.status, ...next } });
	}

	// each key its own gesture: keys never wait for one another (the pacing is remote.ts's)
	let sent = 0;
	const run = (send: () => Promise<unknown>) => pad.run(send, undefined, `key-${++sent}`);

	// ── last order: what an assumed-state tile says instead of a state (§ 2, case 3) ──
	const ORDER_KEY = 'maison-tv-last-order';
	function storedOrder(): { on: boolean; at: number } | null {
		try {
			return JSON.parse(localStorage.getItem(ORDER_KEY) ?? 'null');
		} catch {
			return null;
		}
	}
	let lastOrder = $state(storedOrder());
	function remember(on: boolean) {
		lastOrder = { on, at: Date.now() };
		try {
			localStorage.setItem(ORDER_KEY, JSON.stringify(lastOrder));
		} catch {
			// private mode: remembered for the visit
		}
	}

	async function sendPower(on: boolean) {
		const r = await tvApi.power(on ? 'on' : 'off');
		remember(on);
		patch({ power: r.power });
		void tv.refresh();
	}

	// ── power, read state: the tile's icon, through Command (30 s for the TV) ──
	let wakeStart = $state(0);
	let now = $state(0);
	/** Waking from deep standby (~20 s): a bar on the elapsed time, « up to 30 s » (§ 2, § 6). */
	const waking = $derived(wakeStart > 0 && power.target === true);
	$effect(() => {
		if (!waking) return;
		const timer = setInterval(() => (now = Date.now()), 250);
		return () => clearInterval(timer);
	});
	const elapsed = $derived(waking ? Math.min(LIMIT.tv, Math.max(0, now - wakeStart)) / 1000 : 0);

	function toggle(on: boolean) {
		haptic(CONFIRM);
		wakeStart = on && status?.power === 'deep_standby' ? (now = Date.now()) : 0;
		void power.run(on, () => sendPower(on));
	}

	// ── power, assumed state: two plain buttons ──
	function assumedPower(on: boolean) {
		haptic(CONFIRM);
		return g.run(() => sendPower(on), undefined, on ? 'on' : 'off');
	}

	// ── volume: steps from the last value asked, so quick taps add up ──
	let pendingLevel = $state<number | undefined>(undefined);
	const shownLevel = $derived(pendingLevel ?? volume?.current ?? 0);

	async function setVolume(level?: number, muted?: boolean) {
		const r = await tvApi.setVolume(level, muted);
		patch({ volume: r.volume });
	}

	async function step(delta: number) {
		if (!volume) return;
		const level = Math.min(volume.max, Math.max(volume.min, shownLevel + delta));
		pendingLevel = level;
		await run(() => setVolume(level));
		if (pendingLevel === level) pendingLevel = undefined;
	}

	function send(k: PadKey): Promise<unknown> {
		if (k === 'volume_up') return step(1);
		if (k === 'volume_down') return step(-1);
		if (k === 'mute') return run(() => setVolume(undefined, !volume?.muted));
		// no re-read after a key: nothing in the status changes, and a read is four calls
		return run(() => tvApi.sendKey(TV_KEY[k]));
	}
	const keys = pacedKeys(send);
	const key = (k: TvKey) => () => run(() => tvApi.sendKey(k));

	function switchToBox() {
		haptic(CONFIRM);
		return g.run(
			() => tvApi.switchToBox(),
			() => {
				ui.toast(m.tv_switched_to_box());
				void tv.refresh();
			},
			'box'
		);
	}
	function ambilight() {
		haptic(CONFIRM);
		return g.run(() => tvApi.ambilight('toggle'), (r) => patch({ ambilight: r.ambilight }), 'ambilight');
	}

	const describe = $derived(
		!configured ? m.tv_not_configured() : assumed ? m.tv_assumed() : status ? POWER_STATE[status.power]() : ''
	);
	const fact = $derived(volume ? (volume.muted ? m.tv_muted() : m.tv_volume_fact({ level: volume.current })) : undefined);
</script>

<section class="group" aria-labelledby="tv-title">
	<div class="group-head">
		<h2 id="tv-title" class="group-title">{m.tv_title()}</h2>
	</div>

	<Loaded value={tv}>
		<div class="tiles">
			{#if status}
			<DeviceTile
				{name}
				icon="tv"
				state={describe}
				on={configured && !assumed ? status.power === 'on' : undefined}
				command={power}
				ontoggle={configured && !assumed ? toggle : undefined}
				warn={assumed}
				{fact}
			>
				{#snippet end()}
					<AdminOnly reason={false}>
						<button
							class="icon-btn"
							bind:this={settingsButton}
							aria-label={m.common_settings_of({ name })}
							aria-expanded={settingsOpen}
							onclick={() => (settingsOpen = !settingsOpen)}><Icon name="settings-2" /></button
						>
					</AdminOnly>
				{/snippet}

				{#if settingsOpen || !configured}
					<AdminOnly reason={configured ? false : m.tv_configure_admin()}>
					<Settings
						fields={[
							{ key: 'host', label: m.tv_host(), placeholder: '192.168.1.52' },
							{ key: 'irBlasterHost', label: m.tv_ir_blaster_host(), placeholder: '192.168.1.73' },
							{ key: 'boxHost', label: m.tv_box_host(), placeholder: '192.168.1.153' }
						]}
						initial={{ ...config }}
						hint={configured ? undefined : m.tv_configure_hint()}
						save={async (v) => {
							await tvApi.setConfig({ ...config, ...v });
							await tv.refresh();
						}}
						saved={m.tv_saved()}
						onsaved={() => {
							settingsOpen = false;
							// the form is gone: back to the button that opened it
							void refocus(settingsButton);
						}}
					/>
					</AdminOnly>
				{/if}

				{#if configured}
					{#if assumed}
						<div class="btn-row">
							<button class="btn" {...pending(g.is('on'))} onclick={() => assumedPower(true)}>
								<Icon name="power" busy={g.is('on')} /><span class="btn-text">{m.action_turn_on()}</span>
							</button>
							<button class="btn" {...pending(g.is('off'))} onclick={() => assumedPower(false)}>
								<Icon name="power" busy={g.is('off')} /><span class="btn-text">{m.action_turn_off()}</span>
							</button>
						</div>
						{#if lastOrder}
							<p class="hint">
								{(lastOrder.on ? m.tv_last_order_on : m.tv_last_order_off)({
									time: clock(lastOrder.at)
								})}
							</p>
						{/if}
						<p class="hint">{m.tv_assumed_hint()}</p>
					{:else if waking}
						<Progress
							label={m.tv_waking()}
							value={elapsed}
							max={LIMIT.tv / 1000}
							valueText={m.tv_waking_elapsed({ seconds: Math.round(elapsed) })}
						/>
					{:else if status.power === 'deep_standby'}
						<!-- deep standby needs infrared and ~20 s: say so before the button looks stuck -->
						<p class="hint">{m.tv_deep_standby_hint()}</p>
					{/if}

					{#if volume}
						<div class="volume">
							<Key k="volume_down" fire={keys.volume_down} />
							<span class="level" aria-hidden="true">{shownLevel}</span>
							<Key k="volume_up" fire={keys.volume_up} />
							<Key k="mute" fire={keys.mute} pressed={volume.muted} />
						</div>
					{/if}

					<div class="actions">
						<button class="btn" {...pending(g.is('box'))} onclick={switchToBox}>
							<Icon name="house" busy={g.is('box')} />{m.tv_switch_to_box()}
						</button>
					</div>

					{#if volume}
						<More>
							<Range
								label={m.tv_volume()}
								value={volume.current}
								min={volume.min}
								max={volume.max}
								valueText={(v) => m.tv_volume_value({ level: v, max: volume.max })}
								live
								send={(v) => {
									haptic(TAP);
									return run(() => setVolume(v));
								}}
							/>
							<Pad label={m.tv_pad({ name })} {keys} under={['back', 'home']} volume>
								{#snippet extra()}
									<Key label={m.tv_key_source()} icon="monitor" size="small" fire={key('source')} />
									<Key label={m.tv_key_play_pause()} icon="play" size="small" fire={key('play_pause')} />
								{/snippet}
							</Pad>
							<div class="actions">
								<button class="btn" aria-pressed={status.ambilight?.power ?? false} {...pending(g.is('ambilight'))} onclick={ambilight}>
									<Icon name="sparkles" busy={g.is('ambilight')} />{m.tv_ambilight()}
								</button>
							</div>
						</More>
					{/if}
				{/if}
			</DeviceTile>
			{/if}
		</div>
	</Loaded>
</section>

<style>
	/* − 18 + : two 56 px keys either side of the value (§ 7), mute after them */
	.volume { display: flex; align-items: center; gap: var(--s-2); }
	.level { min-width: 3ch; text-align: center; font: var(--t-group); font-variant-numeric: tabular-nums; }
	.volume :global(.key:last-child) { margin-left: auto; }
</style>
