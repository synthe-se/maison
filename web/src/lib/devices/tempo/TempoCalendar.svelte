<script lang="ts">
	// A season's days, one month at a time (docs/ux.md § 8): a plain <table>
	// (nothing to select, so no grid role), a caption naming the month, abbreviated weekday
	// headers with their full name in `abbr`, each cell read as « mardi 12 novembre, rouge »,
	// today marked `aria-current="date"`, two labelled buttons and Page Up / Page Down (Shift: a
	// year) while the table has focus; at either end the buttons stay, not operable, saying why.
	// The legend stays above with the season's counters in words. Weeks start on Monday, as in
	// France. Each season is read by its own SeasonData, keyed: changing season stops the old
	// one's polling, and the table keeps the focus.
	import { m } from '#lib/paraglide/messages.js';
	import type { Live } from '#lib/live.svelte.ts';
	import { date, isoDay, longDay, percent, weekday } from '#lib/i18n.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { unavailable } from '#lib/gesture.svelte.ts';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import type { TempoCalendar, TempoCalendarDay } from './api.ts';
	import { TEMPO, TEMPO_COLORS, UNSURE, dayWords } from './colors.ts';
	import { seasonOf } from './dates.ts';
	import SeasonData from './SeasonData.svelte';
	import Swatch from './Swatch.svelte';

	/** RTE's Tempo data starts with this season (cache/tempo/ holds every one since). */
	const FIRST_SEASON_YEAR = 2014;
	/** Forecasts reach a week ahead: no month beyond that one. */
	const FORECAST_DAYS = 7;

	const now = new Date();
	const todayIso = isoDay(now);
	const index = (y: number, mo: number) => y * 12 + mo;
	const MIN = index(FIRST_SEASON_YEAR, 8);
	const lastDay = new Date(now.getFullYear(), now.getMonth(), now.getDate() + FORECAST_DAYS);
	const MAX = index(lastDay.getFullYear(), lastDay.getMonth());
	const thisMonth = index(now.getFullYear(), now.getMonth());
	const currentSeason = seasonOf(now.getFullYear(), now.getMonth());
	const seasons = Array.from({ length: Number(currentSeason.slice(0, 4)) - FIRST_SEASON_YEAR + 1 }, (_, i) =>
		seasonOf(FIRST_SEASON_YEAR + i, 8)
	).toReversed();

	let month = $state(thisMonth);
	const year = $derived(Math.floor(month / 12));
	const mo = $derived(month % 12);
	const season = $derived(seasonOf(year, mo));
	const caption = (i: number) => date(new Date(Math.floor(i / 12), i % 12, 1), { month: 'long', year: 'numeric' });

	let cal = $state.raw<Live<TempoCalendar>>();
	const id = $props.id();
	const data = $derived(cal?.data);
	const byDate = $derived(new Map((data?.calendar ?? []).map((d) => [d.date, d])));

	// the weekday headers in the reader's language, Monday first
	const weekdays = $derived(Array.from({ length: 7 }, (_, i) => ({ short: weekday(i, 'short'), long: weekday(i, 'long') })));

	const weeks = $derived.by(() => {
		const lead = (new Date(year, mo, 1).getDay() + 6) % 7;
		const count = new Date(year, mo + 1, 0).getDate();
		const cells: (string | null)[] = Array.from({ length: lead }, () => null);
		for (let d = 1; d <= count; d++) cells.push(isoDay(new Date(year, mo, d)));
		while (cells.length % 7) cells.push(null);
		return Array.from({ length: cells.length / 7 }, (_, w) => cells.slice(w * 7, w * 7 + 7));
	});

	function go(to: number) {
		const next = Math.min(MAX, Math.max(MIN, to));
		if (next === month) return;
		month = next;
		ui.say(caption(next));
	}

	function onkeydown(e: KeyboardEvent) {
		const step = e.shiftKey ? 12 : 1;
		if (e.key === 'PageUp') go(month - step);
		else if (e.key === 'PageDown') go(month + step);
		else return;
		e.preventDefault();
	}

	function pickSeason(s: string) {
		go(s === currentSeason ? thisMonth : index(Number(s.slice(0, 4)), 8));
	}

	const words = (d: TempoCalendarDay | undefined) => (d?.color ? dayWords(d.color, d.isPrediction, d.confidence) : '');
	const stock = $derived(data?.stock);
	const forecasts = $derived(data?.calendar.filter((d) => d.isPrediction).length ?? 0);
	const legend = $derived({
		BLUE: stock ? m.tempo_legend_blue({ count: stock.blue.used }) : TEMPO.BLUE.name(),
		WHITE: stock ? m.tempo_legend_white({ count: stock.white.used, total: stock.white.total }) : TEMPO.WHITE.name(),
		RED: stock ? m.tempo_legend_red({ count: stock.red.used, total: stock.red.total }) : TEMPO.RED.name()
	});
</script>

{#key season}<SeasonData {season} bind:value={cal} />{/key}

<Group id="tempo-calendar-title" title={m.tempo_calendar_title()}>
	{#snippet actions()}
		<div class="season">
			<Select
				hideLabel
				label={m.tempo_season_label()}
				value={season}
				options={seasons.map((s) => ({ value: s, label: m.tempo_season({ season: s }) }))}
				onchange={pickSeason}
			/>
		</div>
	{/snippet}

	<ul class="legend plain-list">
		{#each TEMPO_COLORS as c (c)}
			<li><Swatch color={c} />{legend[c]}</li>
		{/each}
		<li>
			<Swatch color="BLUE" forecast />
			{stock ? m.tempo_legend_forecast({ count: forecasts }) : m.tempo_calendar_prediction()}
		</li>
	</ul>

	<div class="nav">
		<button class="btn" aria-label={m.tempo_prev_month()} {...unavailable(month <= MIN && `${id}-first`)} onclick={() => go(month - 1)}>
			<Icon name="chevron-left" />
		</button>
		<!-- always there: pressed on another month, it does not vanish under the focus -->
		<button class="btn" aria-disabled={month === thisMonth ? 'true' : undefined} onclick={() => go(thisMonth)}>{m.day_today()}</button>
		<button class="btn" aria-label={m.tempo_next_month()} {...unavailable(month >= MAX && `${id}-last`)} onclick={() => go(month + 1)}>
			<Icon name="chevron-right" />
		</button>
		<span class="sr-only" id="{id}-first">{m.tempo_calendar_first({ year: FIRST_SEASON_YEAR })}</span>
		<span class="sr-only" id="{id}-last">{m.tempo_calendar_last()}</span>
	</div>

	<!-- Page Up / Page Down need the table to hold focus (APG date picker) -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
	<table tabindex="0" {onkeydown} aria-keyshortcuts="PageUp PageDown Shift+PageUp Shift+PageDown" aria-describedby="tempo-calendar-info">
		<caption>{caption(month)}</caption>
		<thead>
			<tr>
				{#each weekdays as w (w.long)}<th scope="col" abbr={w.long}>{w.short}</th>{/each}
			</tr>
		</thead>
		<tbody>
			{#each weeks as week, i (i)}
				<tr>
					{#each week as iso, j (iso ?? `pad-${j}`)}
						{#if iso}
							{@const d = byDate.get(iso)}
							{@const isToday = iso === todayIso}
							<td aria-current={isToday ? 'date' : undefined}>
								<Swatch
									color={d?.color}
									forecast={d?.isPrediction}
									unsure={d?.isPrediction && (d.confidence ?? 0) < UNSURE}
									size="cell"
									current={isToday}
								>
									<span>{Number(iso.slice(8))}</span>
									{#if d?.isPrediction && d.confidence !== undefined}<span class="pct">{percent(d.confidence)}</span>{/if}
								</Swatch>
								<span class="sr-only">{d?.color ? m.tempo_day_label({ date: longDay(iso), state: words(d) }) : longDay(iso)}</span>
							</td>
						{:else}
							<td></td>
						{/if}
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
	{#if cal?.loading}<p class="hint">{m.common_loading()}</p>{/if}

	<p class="hint" id="tempo-calendar-info">{m.tempo_calendar_info()}</p>
</Group>

<style>
	.season {
		min-width: 12rem;
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-2) var(--s-4);
		font: var(--t-secondary);
	}
	.legend li {
		display: inline-flex;
		align-items: center;
		gap: var(--s-2);
		font-variant-numeric: tabular-nums;
	}
	.nav {
		display: flex;
		gap: var(--s-2);
	}
	.nav .btn {
		min-height: var(--control-h);
		min-width: var(--control-h);
		justify-content: center;
	}
	table {
		width: 100%;
		max-width: 28rem;
		border-collapse: collapse;
		table-layout: fixed;
	}
	caption {
		font: var(--t-label);
		text-align: left;
		padding-bottom: var(--s-2);
	}
	caption::first-letter {
		text-transform: uppercase;
	}
	th {
		font: var(--t-meta);
		color: var(--ink-muted);
		padding-bottom: var(--s-1);
	}
	td {
		padding: var(--s-half);
		text-align: center;
		vertical-align: middle;
	}
	td :global(.swatch) {
		margin-inline: auto;
	}
	/* the forecast's probability: small, never below 12 px, never faded */
	.pct {
		font: var(--t-tiny);
		margin-top: var(--s-half);
	}
</style>
