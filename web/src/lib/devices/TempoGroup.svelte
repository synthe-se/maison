<script lang="ts">
	// EDF Tempo on the dashboard (docs/ux.md § 8): the color and the price in force, then
	// tomorrow; the tile leads to the Tempo page (calendar, forecasts, days left, prices). Its
	// place is kept while it is first read and when the server does not answer (Loaded).
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import Group from '#lib/components/Group.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { today } from './tempo/data.ts';
	import TempoNow from './tempo/TempoNow.svelte';

	const tempo = live(today);
</script>

<Group id="tempo-title" title={m.nav_tempo()}>
	<Loaded value={tempo} skeletons={1}>
		{#if tempo.data}<div class="tiles"><TempoNow data={tempo.data} /></div>{/if}
	</Loaded>
</Group>
