<script lang="ts" generics="T">
	// A dashboard group of lamps of one family: title, « 2 joignables sur 3 », the family's
	// actions, then one tile per lamp (or why there is none, or that the list could not be read).
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { Live } from '#lib/live.svelte.ts';
	import Loaded from '#lib/components/Loaded.svelte';
	import LampTile from './LampTile.svelte';
	import type { Lamp, LampDriver } from './lamp.ts';

	interface Props {
		title: string;
		/** The family's list, as read (its loading and failure shown here). */
		list: Live<T>;
		lamps: Lamp[];
		reachable: number;
		driver: LampDriver;
		empty: string;
		emptyHint: string;
		/** Buttons in the group head (scan, pairing). */
		actions?: Snippet;
		/** What unfolds under the head (the pairing panel). */
		children?: Snippet;
	}
	let { title, list, lamps, reachable, driver, empty, emptyHint, actions, children }: Props = $props();
	const id = $props.id();
</script>

<section class="group" aria-labelledby="{id}-title">
	<div class="group-head head">
		<h2 id="{id}-title" class="group-title" tabindex="-1">{title}</h2>
		{#if lamps.length}<span class="fact">{m.lamps_reachable_of({ count: reachable, total: lamps.length })}</span>{/if}
		{#if actions}<div class="actions">{@render actions()}</div>{/if}
	</div>

	{@render children?.()}

	<Loaded value={list} empty={lamps.length === 0} emptyText={empty} {emptyHint}>
		<div class="tiles">
			{#each lamps as lamp (lamp.id)}
				<LampTile {lamp} {driver} />
			{/each}
		</div>
	</Loaded>
</section>

<style>
	/* on a phone the actions go under the title rather than squeezing it */
	.head { flex-wrap: wrap; }
	.head .actions { margin-left: auto; }
	.head .fact + .actions { margin-left: 0; }
	.fact { font-variant-numeric: tabular-nums; }
</style>
