<script lang="ts">
	// « Télé » on the dashboard: the TV and the Android box in one group, one framed list (each
	// row leads to its own page, /tv and /androidtv). The remote's keymap is not here: it has
	// its own destination, « Télécommande » (docs/ux.md § 5).
	import { m } from '#lib/paraglide/messages.js';
	import Group from '#lib/components/Group.svelte';
	import TvTile from './tv/TvTile.svelte';
	import BoxTile from './tv/BoxTile.svelte';
</script>

<Group id="tv-title" title={m.tv_group_title()}>
	<div class="pair">
		<TvTile />
		<BoxTile />
	</div>
</Group>

<style>
	/* each device loads on its own (its own skeleton or retry), yet the two read as one list:
	   the TV's frame opens onto the box's, whose top edge is the line between the rows */
	.pair {
		display: grid;
	}
	.pair > :global(.tiles:has(~ .tiles)) {
		border-bottom: 0;
		border-end-start-radius: 0;
		border-end-end-radius: 0;
	}
	.pair > :global(.tiles ~ .tiles) {
		border-start-start-radius: 0;
		border-start-end-radius: 0;
	}
</style>
