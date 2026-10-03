<script lang="ts">
	// The volume on a remote's page (docs/ux.md § 7): − and + either side of the level, mute
	// after them (a pressed toggle when the set says it is muted), and on the TV a slider that
	// follows the finger. The keys are the pad's own senders: a hold paces them alike.
	import { m } from '#lib/paraglide/messages.js';
	import { haptic, TAP } from '#lib/haptics.ts';
	import Range from '#lib/components/Range.svelte';
	import type { TvVolume } from './api.ts';
	import Key from './Key.svelte';
	import type { Fire, PadKey } from './remote.ts';

	interface Props {
		keys: Record<PadKey, Fire>;
		/** What the set says (the TV): the level shown, muted, and the slider. */
		volume?: TvVolume;
		/** The level shown while steps travel (the last asked). */
		level?: number;
		/** Sends a level from the slider (it rejects on failure). */
		set?: (level: number) => Promise<unknown>;
		/** The id of what says why the keys cannot act now. */
		reason?: string;
	}
	let { keys, volume, level, set, reason }: Props = $props();
	const id = $props.id();
</script>

<section class="tile" aria-labelledby="{id}-title">
	<h2 id="{id}-title" class="group-title">{m.tv_volume()}</h2>
	<div class="keys">
		<Key k="volume_down" fire={keys.volume_down} {reason} />
		{#if volume}<span class="level" aria-hidden="true">{level ?? volume.current}</span>{/if}
		<Key k="volume_up" fire={keys.volume_up} {reason} />
		<Key k="mute" fire={keys.mute} pressed={volume?.muted} {reason} />
	</div>
	{#if volume && set}
		{@const v = volume}
		<Range
			label={m.tv_volume()}
			hideLabel
			value={v.current}
			min={v.min}
			max={v.max}
			valueText={(x) => m.tv_volume_value({ level: x, max: v.max })}
			live
			send={(x) => {
				haptic(TAP);
				return set(x);
			}}
		/>
	{/if}
</section>

<style>
	/* − 18 + : two 56 px keys either side of the value (§ 7), mute after them */
	.keys {
		display: flex;
		align-items: center;
		gap: var(--s-2);
	}
	.level {
		min-width: 3ch;
		text-align: center;
		font: var(--t-group);
		font-variant-numeric: tabular-nums;
	}
	.keys :global(.key:last-child) {
		margin-left: auto;
	}
</style>
