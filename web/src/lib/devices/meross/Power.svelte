<script lang="ts">
	// A plug's live power, read while this is mounted: a view that must not ask a plug that does
	// not answer mounts it only while the plug answers (its tile, its « Maintenant » chip).
	// `children` gets the watts, undefined until read.
	import type { Snippet } from 'svelte';
	import { live } from '#lib/live.svelte.ts';
	import { electricity } from './data.ts';
	import { reading } from './units.ts';

	let { id, children }: { id: string; children: Snippet<[watts: number | undefined]> } = $props();
	// svelte-ignore state_referenced_locally (one per plug: the parent keys the list)
	const elec = live(electricity(id));
	const watts = $derived(elec.data ? reading(elec.data.electricity.raw).watts : undefined);
</script>

{@render children(watts)}
