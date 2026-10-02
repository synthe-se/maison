<script lang="ts">
	// Today and tomorrow, the next forecasts and the day's tariff. Mounted only once the server
	// has Tempo, so the forecasts are only asked then (as before).
	import { m } from '#lib/paraglide/messages.js';
	import type { TempoData, TempoPrediction } from '#lib/api.ts';
	import { num } from '#lib/i18n.svelte.ts';
	import { TEMPO, TEMPO_COLORS, dayWords, type TempoColor } from './colors.ts';
	import { dayFromToday, shortDay } from '#lib/i18n.svelte.ts';
	import { tempoForecasts } from './data.ts';
	import Swatch from './Swatch.svelte';

	let { data }: { data: TempoData } = $props();

	/** RTE sets tomorrow's colour at 10:30 (minutes after midnight). */
	const ANNOUNCED_AT = 10 * 60 + 30;
	/** Forecasts shown on the tile beyond tomorrow; the rest is on the Tempo page. */
	const UPCOMING = 4;

	const forecasts = tempoForecasts();
	const predictions = $derived(forecasts.data?.predictions ?? []);

	type Day = { iso: string; color: TempoColor | null; forecast: boolean; p?: number };

	function day(d: TempoData['today']): Day {
		const iso = d?.date.slice(0, 10) ?? '';
		if (d?.color) return { iso, color: d.color, forecast: false };
		const f = predictions.find((p) => p.date === iso);
		return f ? fromForecast(f) : { iso, color: null, forecast: false };
	}
	const fromForecast = (f: TempoPrediction): Day => ({ iso: f.date, color: f.predicted_color, forecast: true, p: f.confidence });

	const today = $derived(day(data.today));
	const tomorrow = $derived(day(data.tomorrow));
	const upcoming = $derived(
		predictions.filter((p) => p.date >= dayFromToday(2)).slice(0, UPCOMING).map(fromForecast)
	);

	// re-read with each answer (every 30 min): before 10:30 tomorrow is not announced yet
	const beforeAnnounce = $derived.by(() => {
		void data;
		const now = new Date();
		return now.getHours() * 60 + now.getMinutes() < ANNOUNCED_AT;
	});
	const tomorrowWords = $derived.by(() => {
		if (tomorrow.color && !tomorrow.forecast) return dayWords(tomorrow.color, false);
		const parts = [beforeAnnounce ? m.tempo_announced_at() : '', tomorrow.color ? dayWords(tomorrow.color, true, tomorrow.p) : ''].filter(Boolean);
		return parts.length ? parts.join(' · ') : dayWords(null, false);
	});

	const tarif = $derived(today.color && data.tarifs ? data.tarifs[TEMPO[today.color].tarif] : null);
	/** €/kWh → « 16,09 c€/kWh » */
	const price = (eurPerKwh: number) => m.tempo_price({ price: num(eurPerKwh * 100, 2) });
</script>

{#snippet row(d: Day, label: string, words: string, sub = '')}
	<li class="day">
		<Swatch color={d.color} forecast={d.forecast} />
		<span class="label">{label}{#if sub}&nbsp;<span class="muted">{sub}</span>{/if}</span>
		<span class="words">{words}</span>
	</li>
{/snippet}

<article class="tile" aria-labelledby="tempo-today-title">
	<div class="tile-head">
		<Swatch color={today.color} forecast={today.forecast} size="tile" />
		<div class="text">
			<h3 class="tile-title" id="tempo-today-title">{m.day_today()}</h3>
			<p class="tile-state">{dayWords(today.color, today.forecast, today.p)}</p>
		</div>
		{#if today.iso}<div class="end"><span class="fact">{shortDay(today.iso)}</span></div>{/if}
	</div>

	<ul class="days">
		{@render row(tomorrow, m.day_tomorrow(), tomorrowWords, tomorrow.iso && shortDay(tomorrow.iso))}
	</ul>

	{#if upcoming.length}
		<div class="block">
			<p class="eyebrow">{m.tempo_prediction_dashboard_title()}</p>
			<ul class="days">
				{#each upcoming as d (d.iso)}
					{@render row(d, shortDay(d.iso), dayWords(d.color, true, d.p))}
				{/each}
			</ul>
		</div>
	{/if}

	{#if tarif}
		<div class="block">
			<p class="eyebrow">
				{m.tempo_current_tarif()}{#if today.forecast}&nbsp;({m.tempo_estimated()}){/if}
			</p>
			<dl class="prices">
				<div><dt>{m.tempo_off_peak()}</dt><dd>{price(tarif.hc)}</dd></div>
				<div><dt>{m.tempo_peak()}</dt><dd>{price(tarif.hp)}</dd></div>
			</dl>
		</div>
	{/if}

	{#if data.tarifs}
		<details class="all">
			<summary class="link-btn quiet">{m.tempo_all_tarifs()}</summary>
			<table>
				<thead>
					<tr>
						<td></td>
						<th scope="col">{m.tempo_off_peak()}</th>
						<th scope="col">{m.tempo_peak()}</th>
					</tr>
				</thead>
				<tbody>
					{#each TEMPO_COLORS as c (c)}
						{@const t = data.tarifs[TEMPO[c].tarif]}
						<tr>
							<th scope="row"><span class="named"><Swatch color={c} />{TEMPO[c].name()}</span></th>
							<td>{price(t.hc)}</td>
							<td>{price(t.hp)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</details>
	{/if}

	{#if data.cached}<p class="hint">{m.tempo_cached()}</p>{/if}
</article>

<style>
	.days { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-2); }
	.day { display: flex; align-items: center; gap: var(--s-2); flex-wrap: wrap; font: var(--t-secondary); }
	.day .label { font-weight: 600; }
	.day .words { margin-left: auto; }
	.block { display: grid; gap: var(--s-2); }
	.prices { display: flex; flex-wrap: wrap; gap: var(--s-1) var(--s-5); margin: 0; }
	.prices dt { font: var(--t-meta); color: var(--ink-muted); }
	.prices dd { margin: 0; font-weight: 600; font-variant-numeric: tabular-nums; }
	.all summary { cursor: pointer; font: var(--t-secondary); min-height: var(--control-h-xs); display: flex; align-items: center; }
	.all table { width: 100%; border-collapse: collapse; margin-top: var(--s-2); font: var(--t-secondary); font-variant-numeric: tabular-nums; }
	.all th, .all td { padding: var(--s-1) var(--s-2); text-align: right; }
	.all th[scope='row'] { text-align: left; font-weight: 500; }
	.all thead th { font: var(--t-meta); color: var(--ink-muted); }
	.named { display: inline-flex; align-items: center; gap: var(--s-2); }
</style>
