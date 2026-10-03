<script lang="ts">
	// Philips Hue lamps over Bluetooth. Hidden when the server runs without Bluetooth (Docker).
	// Looking for new lamps is an admin's.
	import { m } from '#lib/paraglide/messages.js';
	import { hueLampsApi } from '#lib/api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import LampGroup from './lamps/LampGroup.svelte';
	import { fromHue, hue, LIST_EVERY } from './lamps/lamp.ts';

	const stats = live(hue.keys.stats, hueLampsApi.stats);
	const list = live(hue.keys.list, hueLampsApi.list, LIST_EVERY);
	const lamps = $derived((list.data?.lamps ?? []).map(fromHue));

	const scanning = new Gesture();
	const scan = () =>
		scanning.run(
			() => hueLampsApi.scan(),
			async () => {
				ui.toast(m.hue_lamps_scan_started());
				await refresh(hue.key);
			}
		);
</script>

{#if stats.data?.disabled !== true}
	<LampGroup
		title={m.hue_lamps_title()}
		{list}
		{lamps}
		reachable={list.data?.connected ?? 0}
		driver={hue}
		empty={m.hue_lamps_no_lamps()}
		emptyHint={m.hue_lamps_no_lamps_hint()}
	>
		{#snippet actions()}
			<AdminOnly reason={false}>
				<button class="btn" {...pending(scanning.is())} onclick={scan}>
					<Icon name="search" busy={scanning.is()} />{m.hue_lamps_scan()}
				</button>
			</AdminOnly>
		{/snippet}
	</LampGroup>
{/if}
