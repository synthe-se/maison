<script lang="ts">
	// A plug on the dashboard, with its live power read every 5 s and what it costs at the
	// Tempo price in force (« 42 W · 0,7 c€/h »); drawing power in a red day's peak hours, its
	// line says so in the warning style (« Allumée en HP rouge · 0,76 €/h »). A plug that does
	// not answer is not asked (its tile stays, saying since when).
	import { m } from '#lib/paraglide/messages.js';
	import type { MerossPlug } from './api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { time } from '#lib/clock.svelte.ts';
	import { today } from '#lib/devices/tempo/data.ts';
	import { priceNow } from '#lib/devices/tempo/price.ts';
	import PlugTile from './PlugTile.svelte';
	import Power from './Power.svelte';
	import { powerAndCost, redPeakCost } from './units.ts';

	let { plug }: { plug: MerossPlug } = $props();
	const tempo = live(today);
	const price = $derived(tempo.data ? priceNow(time.now, tempo.data) : null);
</script>

{#if plug.isOnline}
	<Power id={plug.id}>
		{#snippet children(w)}
			{@const cost = redPeakCost(plug.isOn, w, price)}
			<PlugTile
				id={plug.id}
				name={plug.name}
				on={plug.isOn}
				online
				lastPing={plug.lastPing}
				fact={w === undefined ? undefined : powerAndCost(w, plug.isOn ? price?.price : undefined)}
				nudge={cost && m.meross_red_peak({ cost })}
			/>
		{/snippet}
	</Power>
{:else}
	<PlugTile id={plug.id} name={plug.name} on={plug.isOn} online={false} lastPing={plug.lastPing} />
{/if}
