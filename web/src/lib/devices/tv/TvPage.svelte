<script lang="ts">
	// The TV's page (/tv): power and the wake-up progress, the volume, the pad, the source and
	// play keys, Ambilight, the box's input, and under its gear its settings (an admin's; open
	// while it is not configured). The dashboard tile keeps the power and the volume − / + only
	// (docs/ux.md § 5, § 7).
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { clock } from '#lib/i18n.svelte.ts';
	import { LIMIT } from '#lib/command.svelte.ts';
	import { pending } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Progress from '#lib/components/Progress.svelte';
	import { tvApi } from './api.ts';
	import Key from './Key.svelte';
	import Pad from './Pad.svelte';
	import Settings from './Settings.svelte';
	import Volume from './Volume.svelte';
	import { TvRemote } from './tv.svelte.ts';

	const tv = new TvRemote();
	const { g } = tv;
	const status = $derived(tv.status);
	const volume = $derived(tv.volume);
	let settingsOpen = $state(false);

	function switchToBox() {
		haptic(CONFIRM);
		return g.run(
			() => tvApi.switchToBox(),
			() => {
				ui.toast(m.tv_switched_to_box());
				void tv.tv.refresh();
			},
			'box'
		);
	}
	function ambilight() {
		haptic(CONFIRM);
		return g.run(
			() => tvApi.ambilight('toggle'),
			(r) => tv.patch({ ambilight: r.ambilight }),
			'ambilight'
		);
	}
</script>

{#snippet settings()}
	<Settings
		fields={[
			{ key: 'host', label: m.tv_host(), placeholder: '192.168.1.52' },
			{ key: 'irBlasterHost', label: m.tv_ir_blaster_host(), placeholder: '192.168.1.73' },
			{ key: 'boxHost', label: m.tv_box_host(), placeholder: '192.168.1.153' }
		]}
		initial={{ ...tv.config }}
		hint={tv.configured ? undefined : m.tv_configure_hint()}
		save={async (v) => {
			await tvApi.setConfig({ ...tv.config, ...v });
			await tv.tv.refresh();
		}}
		saved={m.tv_saved()}
		onsaved={() => (settingsOpen = false)}
	/>
{/snippet}

<PageHead back title={tv.tv.data ? tv.name : m.tv_title()} />

<div class="measure body">
	<Loaded value={tv.tv}>
		{#if status}
			<DeviceTile
				level={2}
				name={tv.name}
				icon="tv"
				state={tv.state}
				on={tv.configured && !tv.assumed ? status.power === 'on' : undefined}
				command={tv.power}
				ontoggle={tv.configured && !tv.assumed ? tv.toggle : undefined}
				warn={tv.assumed}
				fact={tv.fact}
				settings={session.admin ? settings : undefined}
				bind:settingsOpen={() => settingsOpen || !tv.configured, (v) => (settingsOpen = v)}
			>
				{#if !tv.configured}<AdminOnly reason={m.tv_configure_admin()} />{/if}
				{#if tv.configured}
					{#if tv.assumed}
						<div class="btn-row">
							<button class="btn" {...pending(g.is('on'))} onclick={() => tv.assumedPower(true)}>
								<Icon name="power" busy={g.is('on')} /><span class="btn-text">{m.action_turn_on()}</span>
							</button>
							<button class="btn" {...pending(g.is('off'))} onclick={() => tv.assumedPower(false)}>
								<Icon name="power" busy={g.is('off')} /><span class="btn-text">{m.action_turn_off()}</span>
							</button>
						</div>
						{#if tv.lastOrder}
							<p class="hint">{(tv.lastOrder.on ? m.tv_last_order_on : m.tv_last_order_off)({ time: clock(tv.lastOrder.at) })}</p>
						{/if}
						<p class="hint">{m.tv_assumed_hint()}</p>
					{:else if tv.waking}
						<Progress
							label={m.tv_waking()}
							value={tv.elapsed}
							max={LIMIT.tv / 1000}
							valueText={m.tv_waking_elapsed({ seconds: Math.round(tv.elapsed) })}
						/>
					{:else if status.power === 'deep_standby'}
						<!-- deep standby needs infrared and ~20 s: say so before the button looks stuck -->
						<p class="hint">{m.tv_deep_standby_hint()}</p>
					{/if}
				{/if}
			</DeviceTile>

			{#if tv.configured}
				{#if volume}
					<Volume keys={tv.keys} {volume} level={tv.shownLevel} set={(v) => tv.setVolume(v)} />
					<Pad label={m.tv_pad({ name: tv.name })} keys={tv.keys} under={['back', 'home']} volume>
						{#snippet extra()}
							<Key label={m.key_source()} icon="monitor" size="small" fire={tv.key('source')} />
							<Key label={m.key_play_pause()} icon="play" size="small" fire={tv.key('play_pause')} />
						{/snippet}
					</Pad>
				{/if}
				<div class="actions">
					<button class="btn" {...pending(g.is('box'))} onclick={switchToBox}>
						<Icon name="house" busy={g.is('box')} />{m.tv_switch_to_box()}
					</button>
					{#if volume}
						<button class="btn" aria-pressed={status.ambilight?.power ?? false} {...pending(g.is('ambilight'))} onclick={ambilight}>
							<Icon name="sparkles" busy={g.is('ambilight')} />{m.tv_ambilight()}
						</button>
					{/if}
				</div>
			{/if}
		{/if}
	</Loaded>
</div>

<style>
	.body {
		display: grid;
		gap: var(--s-5);
	}
	.actions :global(.btn) {
		min-height: var(--control-h);
	}
</style>
