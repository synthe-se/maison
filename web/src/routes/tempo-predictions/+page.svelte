<script lang="ts">
	import PageHead from '#lib/components/PageHead.svelte';
	// The Tempo destination (docs/ux/tableau-de-bord.md § 5 and § 8): the season's calendar, its
	// stock of white and red days, the week's forecasts and how they are made.
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import type { TempoColor } from '#lib/devices/tempo/colors.ts';
	import { tempoForecasts, tempoState } from '#lib/devices/tempo/data.ts';
	import Swatch from '#lib/devices/tempo/Swatch.svelte';
	import Share from '#lib/devices/tempo/Share.svelte';
	import TempoCalendar from '#lib/devices/tempo/TempoCalendar.svelte';
	import ForecastCard from '#lib/devices/tempo/ForecastCard.svelte';

	const forecasts = tempoForecasts();
	const seasonState = tempoState();

	const loading = $derived(forecasts.loading || seasonState.loading);
	const failed = $derived(
		!loading && (!!forecasts.error || forecasts.data?.success === false || seasonState.data?.success === false)
	);
	const data = $derived(forecasts.data?.success ? forecasts.data : undefined);
</script>

<PageHead title={m.tempo_prediction_title()}>
	{#snippet sub()}{m.tempo_prediction_subtitle()}{/snippet}
	{#snippet end()}
		{#if data?.model_version}<span class="chip"><Icon name="zap" size={14} />{m.tempo_model({ version: data.model_version })}</span>{/if}
	{/snippet}
</PageHead>

{#snippet stock(color: TempoColor, label: string, remaining: number, total: number)}
	{@const used = total - remaining}
	<div class="stock">
		<div class="stock-head">
			<Swatch {color} />
			<span class="label">{label}</span>
			<span class="fact">{m.tempo_stock_used({ count: used, total })}</span>
		</div>
		<Share {color} share={total ? used / total : 0} />
		<p class="hint">{m.tempo_stock_remaining({ count: remaining })}</p>
	</div>
{/snippet}

{#if failed}
	<div class="callout warn" role="alert">
		<p><Icon name="circle-alert" />{forecasts.data?.error || seasonState.data?.error || m.tempo_prediction_error()}</p>
		<p class="hint">{m.tempo_prediction_error_hint()}</p>
	</div>
{:else if loading}
	<p class="hint" role="status">{m.common_loading()}</p>
{/if}

{#if data}
	<div class="sections">
		<div class="columns">
			<TempoCalendar />

			{#if data.state}
				{@const s = data.state}
				<section class="group" aria-labelledby="tempo-season-title">
					<div class="group-head"><h2 id="tempo-season-title" class="group-title">{m.tempo_season({ season: s.season })}</h2></div>
					{@render stock('RED', m.tempo_prediction_red_days(), s.stock_red_remaining, s.stock_red_total)}
					{@render stock('WHITE', m.tempo_prediction_white_days(), s.stock_white_remaining, s.stock_white_total)}
				</section>
			{/if}

			<section class="group" aria-labelledby="tempo-how-title">
				<div class="group-head"><h2 id="tempo-how-title" class="group-title">{m.tempo_prediction_how_it_works()}</h2></div>
				<p class="hint measure">{m.tempo_prediction_explanation()}</p>
			</section>
		</div>

		{#if data.predictions?.length}
			<section class="group" aria-labelledby="tempo-week-title">
				<div class="group-head"><h2 id="tempo-week-title" class="group-title">{m.tempo_prediction_week_forecast()}</h2></div>
				<div class="tiles">
					{#each data.predictions as p (p.date)}<ForecastCard {p} />{/each}
				</div>
			</section>
		{/if}
	</div>
{/if}

<style>
	.sections { display: grid; gap: var(--s-6); }
	.callout p:first-child { display: flex; align-items: center; gap: var(--s-2); }
	.stock { display: grid; gap: var(--s-1); }
	.stock-head { display: flex; align-items: center; gap: var(--s-2); font: var(--t-secondary); }
	.stock-head .label { font-weight: 600; }
	.stock-head .fact { margin-left: auto; font-variant-numeric: tabular-nums; }
</style>
