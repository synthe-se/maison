<script lang="ts">
	// A lamp's page, Hue or Zigbee: back link, its name as the title, the same tile as on the
	// dashboard, color, details, then what only this family has (rename, hide). Not read yet,
	// it says so with a retry; a lamp the server does not know (404) is « not found ».
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { when } from '#lib/i18n.svelte.ts';
	import { ApiError } from '#lib/api.ts';
	import type { Loadable } from '#lib/live.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import StatusRow from '#lib/components/StatusRow.svelte';
	import LampTile from './LampTile.svelte';
	import LampTemperature from './LampTemperature.svelte';
	import type { Lamp, LampDriver } from './lamp.ts';

	interface Props {
		lamp: Lamp | undefined;
		/** The lamp's live value (loading, failed, old). */
		status: Loadable;
		driver: LampDriver;
		/** Rows of the details list that only this family has. */
		rows?: [string, string][];
		/** Replaces the plain temperature slider (Zigbee: white or color). */
		color?: Snippet<[Lamp]>;
		/** Callouts under the tile (Hue: why it is not connected). */
		notes?: Snippet<[Lamp]>;
		/** Sections after the details (rename, hide). */
		children?: Snippet<[Lamp]>;
	}
	let { lamp, status, driver, rows = [], color, notes, children }: Props = $props();
	const missing = $derived(status.error instanceof ApiError && status.error.status === 404);
	const title = $derived(
		lamp?.name ?? (status.loading ? m.common_loading() : missing || !status.failed ? m.lamps_not_found() : m.lamps_lamp())
	);

	const details = $derived<[string, string][]>(
		lamp
			? [
					[m.device_model(), lamp.model ?? m.lamps_unknown_model()],
					[m.lamps_manufacturer(), lamp.manufacturer],
					[m.lamps_radio(), driver.radio()],
					// only the Hue lamps say their firmware
					...(lamp.firmware ? [[m.device_firmware(), lamp.firmware] as [string, string]] : []),
					...rows,
					[m.lamps_last_seen(), lamp.lastSeen ? when(lamp.lastSeen) : m.common_unknown()]
				]
			: []
	);
</script>

<PageHead back {title} />

<div class="measure">
	<Loaded value={status} missing={m.lamps_not_found()} empty={!lamp} emptyText={m.lamps_not_found()}>
		{#if lamp}
			<div class="body">
				<LampTile {lamp} {driver} link={false} />
				{@render notes?.(lamp)}

				{#if color}
					{@render color(lamp)}
				{:else if lamp.temperature !== null}
					<section class="tile" aria-labelledby="lamp-temperature">
						<h2 id="lamp-temperature" class="group-title">{m.lamps_temperature()}</h2>
						<LampTemperature lamp={{ ...lamp, temperature: lamp.temperature }} {driver} />
					</section>
				{/if}

				<section aria-labelledby="lamp-details">
					<h2 id="lamp-details" class="group-title">{m.lamps_device_info()}</h2>
					<dl class="facts">
						{#each details as [label, value] (label)}
							<StatusRow {label} {value} />
						{/each}
					</dl>
				</section>

				{@render children?.(lamp)}
			</div>
		{/if}
	</Loaded>
</div>

<style>
	.body {
		display: grid;
		gap: var(--s-6);
	}
	section {
		display: grid;
		gap: var(--s-3);
	}
</style>
