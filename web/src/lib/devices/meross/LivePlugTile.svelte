<script lang="ts">
	// A plug on the dashboard, with its live power read every 5 s like the old card; a plug that
	// does not answer is not asked (its tile stays, saying since when).
	import { merossApi, type MerossPlug } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import PlugTile from './PlugTile.svelte';
	import { ELECTRICITY_EVERY_MS, electricityKey } from './keys.ts';
	import { livePower } from './units.ts';

	let { plug }: { plug: MerossPlug } = $props();
	// svelte-ignore state_referenced_locally (one tile per plug id: the parent keys the list)
	const id = plug.id;
	const elec = live(electricityKey(id), () => (plug.isOnline ? merossApi.electricity(id) : Promise.resolve(undefined)), ELECTRICITY_EVERY_MS);
	const fact = $derived(plug.isOnline && elec.data ? livePower(elec.data.electricity) : undefined);
</script>

<PlugTile id={plug.id} name={plug.name} on={plug.isOn} online={plug.isOnline} lastPing={plug.lastPing} {fact} />
