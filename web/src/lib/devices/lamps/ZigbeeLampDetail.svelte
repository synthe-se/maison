<script lang="ts">
	// A Zigbee lamp's page: the shared lamp page, plus white/colour, network details and renaming.
	import { untrack } from 'svelte';
	import Tabs from '#lib/components/Tabs.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi } from '#lib/api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import RenameField from '#lib/components/RenameField.svelte';
	import LampPage from './LampPage.svelte';
	import LampTemperature from './LampTemperature.svelte';
	import ZigbeeColour from './ZigbeeColour.svelte';
	import { DETAIL_EVERY, fromZigbee, zigbee, type Lamp } from './lamp.ts';

	/** Zigbee ColorMode attribute: 2 is colour temperature. */
	const COLOR_MODE_TEMPERATURE = 2;

	let { id }: { id: string } = $props();
	const status = live(untrack(() => zigbee.keys.detail(id)), () => zigbeeLampsApi.status(id), DETAIL_EVERY);
	const raw = $derived(status.data?.lamp);
	const lamp = $derived(raw ? fromZigbee(raw) : undefined);

	const rows = $derived<[string, string][]>(
		raw
			? [
					[m.zigbee_lamps_friendly_name(), raw.friendlyName],
					[m.zigbee_lamps_address(), raw.address],
					[m.zigbee_lamps_interview(), raw.interviewCompleted ? m.zigbee_lamps_interview_complete() : m.zigbee_lamps_interview_pending()]
				]
			: []
	);

	// the tab the person chose wins over the lamp's mode, so a poll does not snap it back
	let chosen = $state<'temperature' | 'color' | null>(null);
	const tab = $derived(chosen ?? (raw?.state.colorMode === COLOR_MODE_TEMPERATURE ? 'temperature' : 'color'));

	const g = new Gesture();

	async function switchTab(next: string, l: Lamp) {
		chosen = next === 'temperature' ? 'temperature' : 'color';
		// back to white: the lamp leaves its colour for the last temperature (as React did)
		const temperature = l.temperature;
		if (chosen === 'temperature' && temperature !== null) {
			await g.run(() => zigbeeLampsApi.temperature(l.id, temperature), () => refresh(zigbee.key), 'white');
		}
	}

</script>

{#snippet colour(l: Lamp)}
	{#if raw}
		{@const off = !l.reachable || !l.isOn}
		<section class="tile colour" aria-labelledby="zigbee-colour">
			<h2 id="zigbee-colour" class="group-title">{m.zigbee_lamps_color()}</h2>
			{#if l.temperature !== null}
				<Tabs
					tabs={[
						{ value: 'temperature', label: m.color_white(), icon: 'sun' },
						{ value: 'color', label: m.zigbee_lamps_color(), icon: 'palette' }
					]}
					value={tab}
					onchange={(v) => switchTab(v, l)}
				>
					{#snippet panel(v)}
						{#if v === 'temperature'}
							<LampTemperature lamp={{ ...l, temperature: l.temperature! }} driver={zigbee} />
						{:else}
							<ZigbeeColour lamp={raw!} disabled={off} />
						{/if}
					{/snippet}
				</Tabs>
			{:else}
				<ZigbeeColour lamp={raw} disabled={off} />
			{/if}
		</section>
	{/if}
{/snippet}

<LampPage {lamp} loading={status.loading} driver={zigbee} {rows} colour={raw?.supportsColor ? colour : undefined}>
	{#snippet children(l: Lamp)}
		<RenameField label={m.zigbee_lamps_name()} value={l.name} save={(name) => zigbeeLampsApi.rename(id, name).then(() => refresh(zigbee.key))} said={(name) => m.common_renamed({ name })} />
	{/snippet}
</LampPage>

<style>

	.colour :global(.tabs-trigger) { display: inline-flex; align-items: center; gap: var(--s-2); }
</style>
