<script lang="ts">
	import PageHead from '#lib/components/PageHead.svelte';
	// A lamp's page, Hue or Zigbee: back link, its name as the title, the same tile as on the
	// dashboard, colour, details, then what only this family has (rename, hide).
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { when } from '#lib/i18n.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import LampTile from './LampTile.svelte';
	import LampTemperature from './LampTemperature.svelte';
	import type { Lamp, LampDriver } from './lamp.ts';

	interface Props {
		lamp: Lamp | undefined;
		loading: boolean;
		driver: LampDriver;
		/** Rows of the details list that only this family has. */
		rows?: [string, string][];
		/** Replaces the plain temperature slider (Zigbee: white or colour). */
		colour?: Snippet<[Lamp]>;
		/** Callouts under the tile (Hue: why it is not connected). */
		notes?: Snippet<[Lamp]>;
		/** Sections after the details (rename, hide). */
		children?: Snippet<[Lamp]>;
	}
	let { lamp, loading, driver, rows = [], colour, notes, children }: Props = $props();

	const details = $derived<[string, string][]>(
		lamp
			? [
					[m.device_model(), lamp.model ?? m.lamps_unknown_model()],
					[m.lamps_manufacturer(), lamp.manufacturer],
					[m.lamps_firmware(), lamp.firmware ?? m.common_unknown()],
					...rows,
					[m.lamps_last_seen(), lamp.lastSeen ? when(lamp.lastSeen) : m.common_unknown()]
				]
			: []
	);
</script>

<PageHead back title={loading ? m.common_loading() : (lamp?.name ?? m.lamps_not_found())} />

{#if lamp}
	<div class="measure body">
		<LampTile {lamp} {driver} link={false} />
		{@render notes?.(lamp)}

		{#if colour}
			{@render colour(lamp)}
		{:else if lamp.temperature !== null}
			<section class="tile" aria-labelledby="lamp-temperature">
				<h2 id="lamp-temperature" class="group-title">{m.lamps_temperature()}</h2>
				<LampTemperature lamp={{ ...lamp, temperature: lamp.temperature }} {driver} />
			</section>
		{/if}

		<section aria-labelledby="lamp-details">
			<h2 id="lamp-details" class="group-title">{m.lamps_device_info()}</h2>
			<dl>
				{#each details as [label, value] (label)}
					<div><dt>{label}</dt><dd>{value}</dd></div>
				{/each}
			</dl>
		</section>

		{@render children?.(lamp)}
	</div>
{/if}

<style>
	.body { display: grid; gap: var(--s-6); }
	section { display: grid; gap: var(--s-3); }
	dl { margin: 0; display: grid; }
	dl div { display: flex; justify-content: space-between; gap: var(--s-4); padding-block: var(--s-2); border-bottom: 1px solid var(--line); }
	dt { color: var(--ink-muted); }
	dd { margin: 0; text-align: right; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
</style>
