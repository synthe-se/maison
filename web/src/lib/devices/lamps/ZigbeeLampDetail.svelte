<script lang="ts">
	// A Zigbee lamp's page: the shared lamp page, plus white/colour, network details and renaming.
	import { untrack } from 'svelte';
	import Tabs from '#lib/components/Tabs.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi } from '#lib/api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import LampPage from './LampPage.svelte';
	import LampTemperature from './LampTemperature.svelte';
	import ZigbeeColour from './ZigbeeColour.svelte';
	import { DETAIL_EVERY, fromZigbee, zigbee, type Lamp } from './lamp.ts';

	/** Zigbee ColorMode attribute: 2 is colour temperature. */
	const COLOR_MODE_TEMPERATURE = 2;

	let { id }: { id: string } = $props();
	const status = live(untrack(() => `${zigbee.key}:${id}`), () => zigbeeLampsApi.status(id), DETAIL_EVERY);
	const raw = $derived(status.data?.lamp);
	const lamp = $derived(raw ? fromZigbee(raw) : undefined);

	const rows = $derived<[string, string][]>(
		raw
			? [
					[m.zigbee_lamps_friendly_name(), raw.friendlyName],
					[m.zigbee_lamps_address(), raw.address],
					[m.zigbee_lamps_link_quality(), raw.linkQuality === null ? m.common_unknown() : String(raw.linkQuality)],
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

	let renameDraft = $state<string | null>(null);
	const draft = $derived(renameDraft ?? raw?.name ?? '');
	function rename(e: SubmitEvent) {
		e.preventDefault();
		// the name sent is what is said: a poll already in flight when the rename left may
		// still carry the old one, so the field keeps the new name until a read shows it
		const name = draft.trim();
		return g.run(
			() => zigbeeLampsApi.rename(id, name),
			async () => {
				ui.say(m.zigbee_lamps_renamed({ name }));
				await refresh(zigbee.key);
				if (raw?.name === name) renameDraft = null;
			},
			'rename'
		);
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
	{#snippet children()}
		<form class="field" onsubmit={rename}>
			<label for="zigbee-rename">{m.zigbee_lamps_name()}</label>
			<div class="actions">
				<input id="zigbee-rename" value={draft} oninput={(e) => (renameDraft = e.currentTarget.value)} required />
				<button class="btn" disabled={g.is('rename') || !draft.trim() || draft.trim() === raw?.name}>
					{#if g.is('rename')}<Icon name="loader-circle" class="spin" />{/if}{m.zigbee_lamps_rename_confirm()}
				</button>
			</div>
		</form>
	{/snippet}
</LampPage>

<style>
	input { flex: 1; min-width: 0; }
	.colour :global(.tabs-trigger) { display: inline-flex; align-items: center; gap: var(--s-2); }
</style>
