<script lang="ts">
	// The dashboard: « Maintenant » (a summary of the house that leads to the groups), the
	// scenes, then the groups in a fixed order, the owner's (docs/ux.md § 5): Tempo, the lamps,
	// the cats' corner, the air conditioning, the shutters, the plugs, the rabbit, then « Télé »
	// (the TV and the box, little used) at the bottom; the IR remote's keymap has its own
	// destination. Spatial memory: nothing reorders itself. Compact on a
	// phone: each group one framed list of 56 px rows (docs/ux.md § 1).
	import { m } from '#lib/paraglide/messages.js';
	import PageHead from '#lib/components/PageHead.svelte';
	import NowStrip from '#lib/devices/now/NowStrip.svelte';
	import ScenesGroup from '#lib/devices/ScenesGroup.svelte';
	import TempoGroup from '#lib/devices/TempoGroup.svelte';
	import LampsGroup from '#lib/devices/LampsGroup.svelte';
	import CatsGroup from '#lib/devices/CatsGroup.svelte';
	import ClimateGroup from '#lib/devices/ClimateGroup.svelte';
	import ShuttersGroup from '#lib/devices/ShuttersGroup.svelte';
	import PlugsGroup from '#lib/devices/PlugsGroup.svelte';
	import NabaztagGroup from '#lib/devices/NabaztagGroup.svelte';
	import TvGroup from '#lib/devices/TvGroup.svelte';
</script>

<PageHead title={m.nav_home()} />

<NowStrip />

<div class="dashboard">
	<ScenesGroup />
	<TempoGroup />
	<LampsGroup />
	<CatsGroup />
	<ClimateGroup />
	<ShuttersGroup />
	<PlugsGroup />
	<NabaztagGroup />
	<TvGroup />
</div>

<style>
	/* groups in columns (Ariane's pelotes, docs/ux.md « Layout »): as many 22rem columns as
	   the container holds, six at most; each group stays whole; reading goes down a column, then
	   the next (WCAG 1.3.2); 12 px between groups in a column, 24 px between columns */
	.dashboard {
		columns: 22rem 6;
		column-gap: var(--s-5);
	}
	.dashboard :global(.group) {
		break-inside: avoid;
		margin: 0 0 var(--s-3);
		gap: var(--s-2);
	}
	.dashboard :global(.group + .group) {
		margin-top: 0;
	}
	.dashboard :global(.group:empty) {
		display: none;
	}
	.dashboard :global(.group-head) {
		min-height: var(--control-h-s);
		margin: 0;
	}
	/* a group's devices: one framed list of rows, 56 px each (a 48 px gesture and 4 px above and
	   below), a line between two; a device with a control below its row grows by it */
	.dashboard :global(.tiles) {
		grid-template-columns: minmax(0, 1fr);
		gap: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-l);
		background: var(--surface);
	}
	.dashboard :global(.tiles > .tile) {
		border: 0;
		border-radius: 0;
		background: none;
		padding: var(--s-1) var(--s-3);
		gap: var(--s-2);
	}
	.dashboard :global(.tiles > .tile:has(> :nth-child(2))) {
		padding-bottom: var(--s-3);
	}
	.dashboard :global(.tiles > .tile + .tile) {
		box-shadow: inset 0 1px var(--line);
	}
	/* a group's actions stay on its title's line on a phone */
	.dashboard :global(.group-head .btn) {
		padding-inline: var(--s-2);
	}
	.dashboard :global(.group-head .actions) {
		gap: var(--s-1);
	}
	.dashboard :global(.tiles > .skeleton) {
		min-height: calc(var(--tile-icon) + 2 * var(--s-1));
	}
</style>
