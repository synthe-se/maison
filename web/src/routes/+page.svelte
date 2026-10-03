<script lang="ts">
	// The dashboard: groups in a fixed order, the owner's (docs/ux/tableau-de-bord.md § 5):
	// Tempo, the lights, the cats' corner, the air conditioning, the shutters, the plugs, the
	// rabbit, then the remotes (little used) at the bottom. Spatial memory: nothing reorders itself.
	import { m } from '#lib/paraglide/messages.js';
	import PageHead from '#lib/components/PageHead.svelte';
	import TempoTile from '#lib/devices/TempoTile.svelte';
	import HueLamps from '#lib/devices/HueLamps.svelte';
	import ZigbeeLamps from '#lib/devices/ZigbeeLamps.svelte';
	import CatDevices from '#lib/devices/CatDevices.svelte';
	import Climate from '#lib/devices/Climate.svelte';
	import Shutters from '#lib/devices/Shutters.svelte';
	import MerossPlugs from '#lib/devices/MerossPlugs.svelte';
	import Nabaztag from '#lib/devices/Nabaztag.svelte';
	import RemoteLink from '#lib/devices/RemoteLink.svelte';
	import Tv from '#lib/devices/Tv.svelte';
	import AndroidTv from '#lib/devices/AndroidTv.svelte';
</script>

<PageHead title={m.nav_home()} />

<div class="dashboard">
	<TempoTile />
	<HueLamps />
	<ZigbeeLamps />
	<CatDevices />
	<Climate />
	<Shutters />
	<MerossPlugs />
	<Nabaztag />
	<RemoteLink />
	<Tv />
	<AndroidTv />
</div>

<style>
	/* groups in columns (Ariane's pelotes, docs/ux/mise-en-page.md § 5): as many 22rem columns as
	   the container holds, six at most; each group stays whole; reading goes down a column, then
	   the next (WCAG 1.3.2); 24 px between groups in a column, 24 px between columns */
	.dashboard { columns: 22rem 6; column-gap: var(--s-5); }
	.dashboard :global(.group) { break-inside: avoid; margin: 0 0 var(--s-5); }
	.dashboard :global(.group:empty) { display: none; }
</style>
