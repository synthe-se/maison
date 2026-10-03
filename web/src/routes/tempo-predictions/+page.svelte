<script lang="ts">
	// The Tempo destination (docs/ux.md § 5 and § 8, docs/tempo.md): the season's
	// calendar, its days used and left, the prices, the week (RTE's days, then Maison's forecast, said not
	// official) and how often that forecast was right on past seasons.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { longDate, percent } from '#lib/i18n.svelte.ts';
	import Group from '#lib/components/Group.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { TEMPO, TEMPO_COLORS } from '#lib/devices/tempo/colors.ts';
	import { forecast as forecastData, today as todayData } from '#lib/devices/tempo/data.ts';
	import TempoPrices from '#lib/devices/tempo/TempoPrices.svelte';
	import Swatch from '#lib/devices/tempo/Swatch.svelte';
	import Share from '#lib/devices/tempo/Share.svelte';
	import TempoCalendar from '#lib/devices/tempo/TempoCalendar.svelte';
	import ForecastCard from '#lib/devices/tempo/ForecastCard.svelte';

	const forecast = live(forecastData);
	const today = live(todayData);
	const data = $derived(forecast.data);
	const backtest = $derived(data?.model?.backtest);
</script>

<PageHead title={m.nav_tempo()}>
	{#snippet sub()}{m.tempo_prediction_subtitle()}{/snippet}
</PageHead>

<div class="sections">
	<div class="columns">
		<TempoCalendar />

		<Loaded value={forecast}>
			{#if data}
				{@const s = data.stock}
				<Group id="tempo-season-title" title={m.tempo_season({ season: s.season })}>
					{#each TEMPO_COLORS as c (c)}
						{@const count = s[TEMPO[c].key]}
						<div class="stock">
							<div class="stock-head">
								<Swatch color={c} />
								<span class="label">{TEMPO[c].name()}</span>
								<span class="fact">{m.tempo_days_used({ count: count.used, total: count.total })}</span>
							</div>
							<Share color={c} share={count.total ? count.used / count.total : 0} />
							<p class="hint">{m.tempo_left({ count: count.remaining, total: count.total })}</p>
						</div>
					{/each}
				</Group>
			{/if}
		</Loaded>

		<Loaded value={today}>
			{#if today.data}<TempoPrices data={today.data} />{/if}
		</Loaded>

		<Group id="tempo-unofficial-title" title={m.tempo_unofficial_title()}>
			<p class="hint measure">{m.tempo_unofficial_text()}</p>
			{#if backtest?.horizons.length}
				<table class="scores">
					<caption>{m.tempo_reliability_title()}</caption>
					<thead>
						<tr>
							<th scope="col">{m.tempo_horizon()}</th>
							<th scope="col">{m.tempo_all_year()}</th>
							<th scope="col">{m.tempo_winter()}</th>
						</tr>
					</thead>
					<tbody>
						{#each backtest.horizons as h (h.horizon)}
							<tr>
								<th scope="row">{m.tempo_horizon_day({ n: h.horizon })}</th>
								<td>{percent(h.accuracy)}</td>
								<td>{percent(h.winterAccuracy)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
				<p class="hint measure">
					{m.tempo_reliability_hint({ seasons: backtest.seasons.join(', '), blue: percent(backtest.horizons[0].alwaysBlue) })}
				</p>
			{/if}
		</Group>
	</div>

	{#if data?.days.length}
		<Group id="tempo-week-title" title={m.tempo_prediction_week_forecast()}>
			{#if data.stale && data.weatherIssued}<p class="hint">{m.tempo_stale({ date: longDate(data.weatherIssued) })}</p>{/if}
			<div class="tiles">
				{#each data.days as d (d.date)}<ForecastCard {d} />{/each}
			</div>
			{#if data.note}<p class="hint">{m.tempo_short()}</p>{/if}
		</Group>
	{/if}
</div>

<style>
	.sections {
		display: grid;
		gap: var(--s-6);
	}
	.stock {
		display: grid;
		gap: var(--s-1);
	}
	.stock-head {
		display: flex;
		align-items: center;
		gap: var(--s-2);
		font: var(--t-secondary);
	}
	.stock-head .label {
		font-weight: 600;
	}
	.stock-head .fact {
		margin-left: auto;
		font-variant-numeric: tabular-nums;
	}
	.scores {
		border-collapse: collapse;
		font: var(--t-secondary);
		font-variant-numeric: tabular-nums;
		max-width: 24rem;
	}
	.scores caption {
		font: var(--t-label);
		text-align: left;
		padding-bottom: var(--s-2);
	}
	.scores th,
	.scores td {
		padding: var(--s-1) var(--s-3);
		text-align: right;
	}
	.scores th[scope='row'] {
		text-align: left;
		font-weight: 500;
	}
	.scores thead th {
		font: var(--t-meta);
		color: var(--ink-muted);
	}
</style>
