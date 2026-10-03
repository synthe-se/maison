<script lang="ts">
	// Tempo on the dashboard, in two lines (docs/ux.md § 8, the words in now.ts): the color and
	// the price in force until when (the tile's name, its link to the Tempo page), then what
	// comes next. The calendar, the forecasts, the days left and every price are on the Tempo
	// page, which the tile leads to.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { time } from '#lib/clock.svelte.ts';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import type { TempoToday } from './api.ts';
	import { forecast } from './data.ts';
	import { nextLine, nowLine } from './now.ts';
	import { priceNow } from './price.ts';
	import Swatch from './Swatch.svelte';

	let { data }: { data: TempoToday } = $props();

	const outlook = live(forecast);
	const current = $derived(priceNow(time.now, data));
	const next = $derived(nextLine(data, current, outlook.data, time.now));
</script>

<DeviceTile name={nowLine(data, current)} href="/tempo-predictions" state={next || m.tempo_open()}>
	{#snippet lead()}<Swatch color={current?.color ?? data.today.color} size="tile" />{/snippet}
</DeviceTile>
