<script lang="ts">
	// The Tempo page's prices (docs/ux.md § 8): the price in force and until when, the other
	// period's, tomorrow's once published, then every rate in a table, since when, and the
	// subscription. The dashboard's tile says only the price in force.
	import { m } from '#lib/paraglide/messages.js';
	import type { TempoToday } from './api.ts';
	import { hhmm, longDate, lower, num } from '#lib/i18n.svelte.ts';
	import Group from '#lib/components/Group.svelte';
	import { time } from '#lib/clock.svelte.ts';
	import { TEMPO, TEMPO_COLORS } from './colors.ts';
	import Swatch from './Swatch.svelte';
	import { periodColor, priceNow } from './price.ts';

	let { data }: { data: TempoToday } = $props();

	const current = $derived(priceNow(time.now, data));
	const tomorrowPrices = $derived(data.tomorrow.color && data.tariffs ? data.tariffs[TEMPO[data.tomorrow.color].key] : null);

	/** €/kWh → « 0,1654 €/kWh » */
	const price = (eurPerKwh: number) => m.tempo_price({ price: num(eurPerKwh, 4) });
</script>

<Group id="tempo-prices-title" title={m.tempo_prices_title()}>
	{#if current}
		<div class="block">
			<p class="eyebrow">{m.tempo_now()}</p>
			<p class="now">
				<Swatch color={current.color} />
				<span>{periodColor(current.period, current.color)} · <strong>{price(current.price)}</strong></span>
				<span class="muted">{m.tempo_until({ time: hhmm(current.until) })}</span>
			</p>
			<p class="hint">{periodColor(current.other.period, current.color)} · {price(current.other.price)}</p>
			{#if tomorrowPrices && data.tomorrow.color}
				<p class="hint">
					{m.tempo_tomorrow_prices({
						color: lower(TEMPO[data.tomorrow.color].name()),
						hc: price(tomorrowPrices.offPeak),
						hp: price(tomorrowPrices.peak)
					})}
				</p>
			{/if}
		</div>
	{/if}

	{#if data.tariffs}
		{@const t = data.tariffs}
		<table>
			<caption class="sr-only">{m.tempo_prices_title()}</caption>
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
						<td>{price(t[TEMPO[c].key].offPeak)}</td>
						<td>{price(t[TEMPO[c].key].peak)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<p class="hint">
			{m.tempo_prices_from({ date: longDate(t.startsOn) })}
			{#if t.subscription}{m.tempo_subscription({ price: num(t.subscription, 2) })}{/if}
		</p>
	{/if}

	{#if data.cached}<p class="hint">{m.tempo_cached()}</p>{/if}
</Group>

<style>
	.now {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--s-1) var(--s-2);
		margin: 0;
		font-variant-numeric: tabular-nums;
	}
	table {
		width: 100%;
		max-width: 24rem;
		border-collapse: collapse;
		font: var(--t-secondary);
		font-variant-numeric: tabular-nums;
	}
	th,
	td {
		padding: var(--s-1) var(--s-2);
		text-align: right;
	}
	th[scope='row'] {
		text-align: left;
		font-weight: 500;
	}
	thead th {
		font: var(--t-meta);
		color: var(--ink-muted);
	}
	.named {
		display: inline-flex;
		align-items: center;
		gap: var(--s-2);
	}
</style>
