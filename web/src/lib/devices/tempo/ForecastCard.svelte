<script lang="ts">
	// One forecast day: its colour in words with its probability, the three probabilities, and
	// what the rules allow that day.
	import { m } from '#lib/paraglide/messages.js';
	import type { TempoPrediction } from '#lib/api.ts';
	import Icon from '#lib/components/Icon.svelte';
	import { TEMPO, TEMPO_COLORS, probable } from './colors.ts';
	import { dayLabel, percent, shortDay } from '#lib/i18n.svelte.ts';
	import Swatch from './Swatch.svelte';
	import Share from './Share.svelte';

	let { p }: { p: TempoPrediction } = $props();
	const id = $props.id();
</script>

<article class="tile" aria-labelledby="{id}-day">
	<div class="tile-head">
		<Swatch color={p.predicted_color} forecast size="tile" />
		<div class="text">
			<h3 class="tile-title day" id="{id}-day">{dayLabel(p.date)}</h3>
			<p class="tile-state">{probable(p.predicted_color, p.confidence)}</p>
		</div>
		<div class="end"><span class="fact">{shortDay(p.date)}</span></div>
	</div>

	<div class="block">
		<p class="eyebrow" id="{id}-probs">{m.tempo_probabilities()}</p>
		<ul class="probs" aria-labelledby="{id}-probs">
			{#each TEMPO_COLORS as c (c)}
				<li>
					<Swatch color={c} />
					<span>{TEMPO[c].name()}</span>
					<Share color={c} share={p.probabilities[c]} />
					<span class="val">{percent(p.probabilities[c])}</span>
				</li>
			{/each}
		</ul>
	</div>

	<div class="actions">
		{#if p.constraints.is_in_red_period}
			<span class="chip"><Icon name="snowflake" size={14} />{m.tempo_prediction_red_period()}</span>
		{:else}
			<span class="chip"><Icon name="sun" size={14} />{m.tempo_prediction_outside_red_period()}</span>
		{/if}
		{#if !p.constraints.can_be_red}
			<span class="chip"><Icon name="circle-alert" size={14} />{m.tempo_prediction_red_blocked()}</span>
		{/if}
		{#if !p.constraints.can_be_white}
			<span class="chip"><Icon name="circle-alert" size={14} />{m.tempo_prediction_white_blocked()}</span>
		{/if}
	</div>
</article>

<style>
	.day::first-letter { text-transform: uppercase; }
	.block { display: grid; gap: var(--s-2); }
	.probs { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-1); font: var(--t-secondary); }
	.probs li { display: grid; grid-template-columns: auto 4.5rem 1fr 3rem; align-items: center; gap: var(--s-2); }
	.val { text-align: right; font-variant-numeric: tabular-nums; }
</style>
