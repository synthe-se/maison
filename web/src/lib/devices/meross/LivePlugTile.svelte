<script lang="ts">
	// A plug that answers: its tile with the live power, read every 5 s like the old card.
	import { merossApi, type MerossPlug } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import PlugTile from './PlugTile.svelte';
	import { ELECTRICITY_EVERY_MS, electricityKey } from './keys.ts';
	import { livePower } from './units.ts';

	let { plug }: { plug: MerossPlug } = $props();
	// svelte-ignore state_referenced_locally (one tile per plug id: the parent keys the list)
	const id = plug.id;
	const elec = live(electricityKey(id), () => merossApi.electricity(id), ELECTRICITY_EVERY_MS);
	const fact = $derived(elec.data ? livePower(elec.data.electricity) : undefined);
</script>

<PlugTile id={plug.id} name={plug.name} on={plug.isOn} online={plug.isOnline} {fact} />
