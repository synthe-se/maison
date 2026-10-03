<script lang="ts">
	// A season's calendar, read while this is mounted: TempoCalendar mounts one per season
	// (keyed), so changing season stops the old one's polling. Draws nothing; the calendar
	// draws `value`.
	import type { Live } from '#lib/live.svelte.ts';
	import type { TempoCalendar } from './api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { calendar } from './data.ts';

	let { season, value = $bindable() }: { season: string; value?: Live<TempoCalendar> } = $props();
	// svelte-ignore state_referenced_locally (keyed by season)
	value = live(calendar(season));
</script>
