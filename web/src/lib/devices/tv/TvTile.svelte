<script lang="ts">
	// The TV's row in the dashboard's « Télé » group: its icon turns it on or off (or, its state
	// assumed, « Allumer » and « Éteindre »), its name leads to its page (/tv: the pad, the
	// volume slider, Ambilight, the settings), and the volume − / + on the row (docs/ux.md § 1, § 7).
	import { m } from '#lib/paraglide/messages.js';
	import Loaded from '#lib/components/Loaded.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import { pending } from '#lib/gesture.svelte.ts';
	import Key from './Key.svelte';
	import { TvRemote } from './tv.svelte.ts';

	const tv = new TvRemote();
	const { g } = tv;
</script>

<Loaded value={tv.tv} skeletons={1}>
	<div class="tiles">
		{#if tv.status}
			<DeviceTile
				name={tv.name}
				icon="tv"
				href="/tv"
				state={tv.state}
				on={tv.configured && !tv.assumed ? tv.status.power === 'on' : undefined}
				command={tv.power}
				ontoggle={tv.configured && !tv.assumed ? tv.toggle : undefined}
				warn={tv.assumed}
			>
				{#snippet end()}
					{#if tv.volume}
						<div class="volume" role="group" aria-label={m.tv_volume()}>
							<Key k="volume_down" size="small" fire={tv.keys.volume_down} />
							<span class="level" aria-hidden="true">{tv.volume.muted ? '–' : tv.shownLevel}</span>
							<Key k="volume_up" size="small" fire={tv.keys.volume_up} />
						</div>
					{/if}
				{/snippet}
				{#if tv.configured && tv.assumed}
					<div class="btn-row">
						<button class="btn" {...pending(g.is('on'))} onclick={() => tv.assumedPower(true)}>
							<Icon name="power" busy={g.is('on')} /><span class="btn-text">{m.action_turn_on()}</span>
						</button>
						<button class="btn" {...pending(g.is('off'))} onclick={() => tv.assumedPower(false)}>
							<Icon name="power" busy={g.is('off')} /><span class="btn-text">{m.action_turn_off()}</span>
						</button>
					</div>
				{/if}
			</DeviceTile>
		{/if}
	</div>
</Loaded>

<style>
	.volume {
		display: flex;
		align-items: center;
		gap: var(--s-1);
	}
	.level {
		min-width: 2.5ch;
		text-align: center;
		font: var(--t-label);
		font-variant-numeric: tabular-nums;
	}
</style>
