<script lang="ts">
	// A Zigbee lamp's page: the shared lamp page, plus white/color, network details and renaming.
	import Tabs from '#lib/components/Tabs.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi } from './api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import RenameField from '#lib/components/RenameField.svelte';
	import LampPage from './LampPage.svelte';
	import LampTemperature from './LampTemperature.svelte';
	import ZigbeeColor from './ZigbeeColor.svelte';
	import { fromZigbee, zigbee, type Lamp } from './lamp.ts';

	/** Zigbee ColorMode attribute: 2 is color temperature. */
	const COLOR_MODE_TEMPERATURE = 2;

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally (the route keys this view by lamp)
	const status = live(zigbee.detail(id));
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
		// back to white: the lamp leaves its color for the last temperature (as React did)
		const temperature = l.temperature;
		if (chosen === 'temperature' && temperature !== null) {
			await g.run(
				() => zigbeeLampsApi.temperature(l.id, temperature),
				() => refresh(zigbee.key),
				'white'
			);
		}
	}
</script>

{#snippet color(l: Lamp)}
	{#if raw}
		<section class="tile" aria-labelledby="zigbee-color">
			<h2 id="zigbee-color" class="group-title">{m.zigbee_lamps_color()}</h2>
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
							<ZigbeeColor lamp={raw!} />
						{/if}
					{/snippet}
				</Tabs>
			{:else}
				<ZigbeeColor lamp={raw} />
			{/if}
		</section>
	{/if}
{/snippet}

<LampPage {lamp} {status} driver={zigbee} {rows} color={raw?.supportsColor ? color : undefined}>
	{#snippet children(l: Lamp)}
		<RenameField
			label={m.zigbee_lamps_name()}
			value={l.name}
			save={(name) => zigbeeLampsApi.rename(id, name).then(() => refresh(zigbee.key))}
			said={(name) => m.common_renamed({ name })}
		/>
	{/snippet}
</LampPage>
