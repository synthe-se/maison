<script lang="ts">
	// EDF Tempo on the dashboard (docs/ux/tableau-de-bord.md § 8). Its place is kept while it is
	// first read and when the server does not answer (Loaded says which).
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { tempoToday } from './tempo/data.ts';
	import TempoToday from './tempo/TempoToday.svelte';

	const tempo = tempoToday();
</script>

<section class="group" aria-labelledby="tempo-title">
	<div class="group-head">
		<h2 id="tempo-title" class="group-title">{m.nav_tempo()}</h2>
		<a class="btn fact-btn" href="/tempo-predictions"><Icon name="calendar" />{m.tempo_open_page()}</a>
	</div>
	<Loaded value={tempo} skeletons={1}>
		{#if tempo.data}<TempoToday data={tempo.data} />{/if}
	</Loaded>
</section>
