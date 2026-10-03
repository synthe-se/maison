<script lang="ts">
	// A plug drawing power in a red day's peak hours (« Lave-linge en HP rouge · 0,76 €/h »), in
	// the strip; nothing otherwise (a plug that does not answer is not asked).
	import { m } from '#lib/paraglide/messages.js';
	import type { MerossPlug } from '#lib/devices/meross/api.ts';
	import type { PriceNow } from '#lib/devices/tempo/price.ts';
	import Power from '#lib/devices/meross/Power.svelte';
	import { redPeakCost } from '#lib/devices/meross/units.ts';
	import Chip from './Chip.svelte';

	let { plug, price }: { plug: MerossPlug; price: PriceNow | null } = $props();
</script>

{#if plug.isOnline}
	<Power id={plug.id}>
		{#snippet children(w)}
			{@const cost = redPeakCost(plug.isOn, w, price)}
			{#if cost}<Chip text={m.now_red_peak({ name: plug.name, cost })} to="meross-title" warn />{/if}
		{/snippet}
	</Power>
{/if}
