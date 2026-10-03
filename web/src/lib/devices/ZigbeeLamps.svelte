<script lang="ts">
	// Zigbee lamps on the server's own coordinator. Hidden when Zigbee is disabled. Pairing new
	// lamps is an admin's.
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import LampGroup from './lamps/LampGroup.svelte';
	import ZigbeePairing from './lamps/ZigbeePairing.svelte';
	import { fromZigbee, LIST_EVERY, zigbee } from './lamps/lamp.ts';

	const stats = live(zigbee.keys.stats, zigbeeLampsApi.stats);
	const list = live(zigbee.keys.list, zigbeeLampsApi.list, LIST_EVERY);
	const lamps = $derived((list.data?.lamps ?? []).map(fromZigbee));

	let pairing = $state(false);
</script>

{#if stats.data?.disabled !== true}
	<LampGroup
		title={m.zigbee_lamps_title()}
		{list}
		{lamps}
		reachable={list.data?.connected ?? 0}
		driver={zigbee}
		empty={m.zigbee_lamps_no_lamps()}
		emptyHint={m.zigbee_lamps_no_lamps_hint()}
	>
		{#snippet actions()}
			<AdminOnly reason={false}>
				<button class="btn" aria-expanded={pairing} onclick={() => (pairing = !pairing)}>
					<Icon name="plus" />{m.action_pair()}
				</button>
			</AdminOnly>
		{/snippet}
		{#if pairing}<ZigbeePairing />{/if}
	</LampGroup>
{/if}
