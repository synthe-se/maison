<script lang="ts">
	// Today and tomorrow (RTE's), the days after (Maison's forecast, said as such), the days
	// left this season and the day's prices. Mounted only once the server has Tempo, so the
	// forecast is only asked then.
	import { m } from '#lib/paraglide/messages.js';
	import type { TempoColor, TempoForecastDay, TempoToday } from '#lib/api.ts';
	import { clock, date, num, shortDay } from '#lib/i18n.svelte.ts';
	import { TEMPO, TEMPO_COLORS, UNSURE, dayWords } from './colors.ts';
	import { tempoForecast } from './data.ts';
	import Swatch from './Swatch.svelte';
	import { priceNow, type Period } from './price.ts';

	let { data }: { data: TempoToday } = $props();

	/** RTE publishes tomorrow's colour around 10:40 (minutes after midnight). */
	const ANNOUNCED_AT = 10 * 60 + 40;

	const forecast = tempoForecast();
	const days = $derived(forecast.data?.days ?? []);

	type Day = { iso: string; color: TempoColor | null; forecast: boolean; p?: number };
	const fromForecast = (d: TempoForecastDay): Day => ({ iso: d.date, color: d.color, forecast: !d.official, p: d.confidence });

	const today: Day = $derived({ iso: data.today.date, color: data.today.color, forecast: false });
	const tomorrow: Day = $derived.by(() => {
		if (data.tomorrow.color) return { iso: data.tomorrow.date, color: data.tomorrow.color, forecast: false };
		const f = days.find((d) => d.date === data.tomorrow.date);
		return f ? fromForecast(f) : { iso: data.tomorrow.date, color: null, forecast: false };
	});
	const upcoming = $derived(days.filter((d) => d.horizon >= 2).map(fromForecast));

	// re-read with each answer (every 30 min): before 10:40 tomorrow is not announced yet
	const beforeAnnounce = $derived.by(() => {
		void data;
		const now = new Date();
		return now.getHours() * 60 + now.getMinutes() < ANNOUNCED_AT;
	});
	const tomorrowWords = $derived.by(() => {
		if (!tomorrow.forecast && tomorrow.color) return dayWords(tomorrow.color, false);
		const parts = [beforeAnnounce ? m.tempo_announced_at() : '', tomorrow.color ? dayWords(tomorrow.color, true, tomorrow.p) : ''].filter(Boolean);
		return parts.length ? parts.join(' · ') : dayWords(null, false);
	});

	// the price in force, recomputed every half minute: the boundaries need no request
	let now = $state(new Date());
	$effect(() => {
		const timer = setInterval(() => (now = new Date()), 30_000);
		return () => clearInterval(timer);
	});
	const current = $derived(priceNow(now, data));
	const tomorrowPrices = $derived(data.tomorrow.color && data.tarifs ? data.tarifs[TEMPO[data.tomorrow.color].key] : null);

	/** €/kWh → « 0,1654 €/kWh » */
	const price = (eurPerKwh: number) => m.tempo_price({ price: num(eurPerKwh, 4) });
	/** « HP bleu » */
	const periodColor = (period: Period, color: TempoColor) =>
		m.tempo_period_color({ period: period === 'hp' ? m.tempo_hp() : m.tempo_hc(), color: TEMPO[color].name().toLocaleLowerCase() });
	/** « 22:00 » as the reader says it */
	const hour = (hhmm: string) => clock(new Date(2000, 0, 1, Number(hhmm.slice(0, 2)), Number(hhmm.slice(3, 5))));
</script>

{#snippet row(d: Day, label: string, words: string, sub = '')}
	<li class="day">
		<Swatch color={d.color} forecast={d.forecast} unsure={d.forecast && (d.p ?? 0) < UNSURE} />
		<span class="label">{label}{#if sub}&nbsp;<span class="muted">{sub}</span>{/if}</span>
		<span class="words">{words}</span>
	</li>
{/snippet}

<article class="tile" aria-labelledby="tempo-today-title">
	<div class="tile-head">
		<Swatch color={today.color} size="tile" />
		<div class="text">
			<h3 class="tile-title" id="tempo-today-title">{m.day_today()}</h3>
			<p class="tile-state">{dayWords(today.color, false)}</p>
		</div>
		{#if today.iso}<div class="end"><span class="fact">{shortDay(today.iso)}</span></div>{/if}
	</div>

	<ul class="days">
		{@render row(tomorrow, m.day_tomorrow(), tomorrowWords, shortDay(tomorrow.iso))}
	</ul>

	{#if upcoming.length}
		<div class="block">
			<p class="eyebrow">{m.tempo_prediction_dashboard_title()}</p>
			<ul class="days">
				{#each upcoming as d (d.iso)}
					{@render row(d, shortDay(d.iso), dayWords(d.color, d.forecast, d.p))}
				{/each}
			</ul>
		</div>
	{/if}

	<div class="block">
		<p class="eyebrow">{m.tempo_remaining_title()}</p>
		<ul class="days">
			{#each TEMPO_COLORS as c (c)}
				{@const count = data.stock[TEMPO[c].key]}
				<li class="day">
					<Swatch color={c} />
					<span class="label">{TEMPO[c].name()}</span>
					<span class="words">{m.tempo_left({ count: count.remaining, total: count.total })}</span>
				</li>
			{/each}
		</ul>
	</div>

	{#if current}
		<div class="block">
			<p class="eyebrow">{m.tempo_now()}</p>
			<p class="now">
				<Swatch color={current.color} />
				<span>{periodColor(current.period, current.color)} · <strong>{price(current.price)}</strong></span>
				<span class="muted">{m.tempo_until({ time: hour(current.until) })}</span>
			</p>
			<p class="hint">{periodColor(current.other.period, current.color)} · {price(current.other.price)}</p>
			{#if tomorrowPrices && data.tomorrow.color}
				<p class="hint">
					{m.tempo_tomorrow_prices({ color: TEMPO[data.tomorrow.color].name().toLocaleLowerCase(), hc: price(tomorrowPrices.hc), hp: price(tomorrowPrices.hp) })}
				</p>
			{/if}
		</div>
	{/if}

	{#if data.tarifs}
		{@const t = data.tarifs}
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
						<tr>
							<th scope="row"><span class="named"><Swatch color={c} />{TEMPO[c].name()}</span></th>
							<td>{price(t[TEMPO[c].key].hc)}</td>
							<td>{price(t[TEMPO[c].key].hp)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="hint">
				{m.tempo_prices_from({ date: date(new Date(`${t.dateDebut}T12:00:00`), { day: 'numeric', month: 'long', year: 'numeric' }) })}
				{#if t.subscription}{m.tempo_subscription({ price: num(t.subscription, 2) })}{/if}
			</p>
		</details>
	{/if}

	{#if data.cached}<p class="hint">{m.tempo_cached()}</p>{/if}
</article>

<style>
	.days { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-2); }
	.day { display: flex; align-items: center; gap: var(--s-2); flex-wrap: wrap; font: var(--t-secondary); }
	.day .label { font-weight: 600; }
	.day .words { margin-left: auto; font-variant-numeric: tabular-nums; }
	.block { display: grid; gap: var(--s-2); }
	.now { display: flex; align-items: center; flex-wrap: wrap; gap: var(--s-1) var(--s-2); margin: 0; font-variant-numeric: tabular-nums; }
	.all summary { cursor: pointer; font: var(--t-secondary); min-height: var(--control-h-xs); display: flex; align-items: center; }
	.all table { width: 100%; border-collapse: collapse; margin-top: var(--s-2); font: var(--t-secondary); font-variant-numeric: tabular-nums; }
	.all th, .all td { padding: var(--s-1) var(--s-2); text-align: right; }
	.all th[scope='row'] { text-align: left; font-weight: 500; }
	.all thead th { font: var(--t-meta); color: var(--ink-muted); }
	.named { display: inline-flex; align-items: center; gap: var(--s-2); }
</style>
