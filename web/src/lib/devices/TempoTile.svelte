<script lang="ts">
	// EDF Tempo on the dashboard (docs/ux/tableau-de-bord.md § 8). Optional: without RTE
	// credentials the server says so and the group is not shown at all.
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import { tempoToday } from './tempo/data.ts';
	import TempoToday from './tempo/TempoToday.svelte';

	const tempo = tempoToday();
</script>

{#if tempo.loading || tempo.data?.success}
	<section class="group" aria-labelledby="tempo-title">
		<div class="group-head">
			<h2 id="tempo-title" class="group-title">{m.tempo_title()}</h2>
			<a class="btn fact-btn" href="/tempo-predictions"><Icon name="calendar" />{m.tempo_open_calendar()}</a>
		</div>
		{#if tempo.data?.success}
			<TempoToday data={tempo.data} />
		{:else}
			<p class="hint" role="status">{m.common_loading()}</p>
		{/if}
	</section>
{/if}

<style>
	.fact-btn { margin-left: auto; }
</style>
