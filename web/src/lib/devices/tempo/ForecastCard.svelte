<script lang="ts">
	// One day of the week: RTE's color once published; else Maison's forecast, its three
	// probabilities as one bar and in words, and how often the forecast was right this far
	// ahead (the backtest of model.json).
	import { m } from '#lib/paraglide/messages.js';
	import type { TempoForecastDay } from '#lib/devices/tempo/api.ts';
	import { dayLabel, percent, shortDay } from '#lib/i18n.svelte.ts';
	import { TEMPO, TEMPO_COLORS, UNSURE, dayWords } from './colors.ts';
	import Swatch from './Swatch.svelte';

	let { d }: { d: TempoForecastDay } = $props();
	const id = $props.id();
</script>

<article class="tile" aria-labelledby="{id}-day">
	<div class="tile-head">
		<Swatch color={d.color} forecast={!d.official} unsure={!d.official && d.confidence < UNSURE} size="tile" />
		<div class="text">
			<h3 class="tile-title day" id="{id}-day">{dayLabel(d.date)}</h3>
			<p class="tile-state">{dayWords(d.color, !d.official, d.confidence)}</p>
		</div>
		<div class="end"><span class="fact">{shortDay(d.date)}</span></div>
	</div>

	{#if d.official}
		<p class="hint">{m.tempo_official()}</p>
	{:else}
		<div class="block">
			<p class="eyebrow" id="{id}-probs">{m.tempo_probabilities()}</p>
			<span class="bar" aria-hidden="true">
				{#each TEMPO_COLORS as c (c)}
					<span class={TEMPO[c].key} style:width="{d.probabilities[c] * 100}%"></span>
				{/each}
			</span>
			<ul class="probs plain-list" aria-labelledby="{id}-probs">
				{#each TEMPO_COLORS as c (c)}
					<li><Swatch color={c} /><span>{TEMPO[c].name()}</span><span class="val">{percent(d.probabilities[c])}</span></li>
				{/each}
			</ul>
		</div>
		{#if d.reliability}
			<p class="hint">
				{m.tempo_reliability_day({ percent: percent(d.reliability.accuracy), n: d.horizon, winter: percent(d.reliability.winterAccuracy) })}
			</p>
		{/if}
	{/if}
</article>

<style>
	.day::first-letter {
		text-transform: uppercase;
	}
	.block {
		display: grid;
		gap: var(--s-2);
	}
	/* the three shares side by side, each in its color and shape (white a ring, red hatched) */
	.bar {
		display: flex;
		height: var(--bar-h);
		border-radius: var(--radius-pill);
		overflow: hidden;
		background: var(--ground-raised);
	}
	.bar span {
		height: 100%;
	}
	.bar .blue {
		background: var(--tempo-blue);
	}
	.bar .white {
		background: var(--tempo-white);
		box-shadow: inset 0 0 0 1.5px var(--tempo-white-ring);
	}
	.bar .red {
		background: var(--tempo-red-hatch);
	}
	.probs {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-1) var(--s-4);
		font: var(--t-secondary);
	}
	.probs li {
		display: inline-flex;
		align-items: center;
		gap: var(--s-2);
	}
	.val {
		font-variant-numeric: tabular-nums;
		font-weight: 600;
	}
</style>
