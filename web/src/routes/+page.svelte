<script lang="ts">
	import PageHead from '#lib/components/PageHead.svelte';
	// The dashboard: groups in a fixed order, most touched first (docs/ux/tableau-de-bord.md § 5).
	// Spatial memory: nothing reorders itself.
	import { m } from '#lib/paraglide/messages.js';
	import Tv from '#lib/devices/Tv.svelte';
	import AndroidTv from '#lib/devices/AndroidTv.svelte';
	import Shutters from '#lib/devices/Shutters.svelte';
	import HueLamps from '#lib/devices/HueLamps.svelte';
	import ZigbeeLamps from '#lib/devices/ZigbeeLamps.svelte';
	import MerossPlugs from '#lib/devices/MerossPlugs.svelte';
	import Climate from '#lib/devices/Climate.svelte';
	import CatDevices from '#lib/devices/CatDevices.svelte';
	import TempoTile from '#lib/devices/TempoTile.svelte';
	import Nabaztag from '#lib/devices/Nabaztag.svelte';
	import RemoteLink from '#lib/devices/RemoteLink.svelte';
</script>

<PageHead title={m.nav_home()} />

<div class="dashboard">
	<Tv />
	<AndroidTv />
	<Shutters />
	<HueLamps />
	<ZigbeeLamps />
	<MerossPlugs />
	<Climate />
	<CatDevices />
	<TempoTile />
	<Nabaztag />
	<RemoteLink />
</div>

<style>
	/* groups in columns (Ariane's pelotes, docs/ux/mise-en-page.md § 5): as many 22rem columns as
	   the container holds, six at most; each group stays whole; reading goes down a column, then
	   the next (WCAG 1.3.2); 24 px between groups in a column, 24 px between columns */
	.dashboard { columns: 22rem 6; column-gap: var(--s-5); }
	.dashboard :global(.group) { break-inside: avoid; margin: 0 0 var(--s-6); }
	.dashboard :global(.group:empty) { display: none; }
</style>
