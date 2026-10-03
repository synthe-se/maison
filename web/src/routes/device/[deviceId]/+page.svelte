<script lang="ts">
	// A cat device's page (feeder, fountain, litter box): reached from its tile, « Retour à
	// l'accueil » at the top (docs/ux.md § 5), one column at --measure. Its local connection is
	// a labelled switch (§ 2, case 2). `/device/feeder` (the app's « Nourrir le chat » shortcut)
	// opens the first device of that kind. The list unread says so with a retry (Loaded), never
	// « not found ».
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import PageHead from '#lib/components/PageHead.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import SettingRow from '#lib/components/SettingRow.svelte';
	import FeederPanel from '#lib/devices/cats/FeederPanel.svelte';
	import FountainPanel from '#lib/devices/cats/FountainPanel.svelte';
	import LitterBoxPanel from '#lib/devices/cats/LitterBoxPanel.svelte';
	import { TYPE_LABEL } from '#lib/devices/cats/tuya.svelte.ts';
	import { devices } from '#lib/devices/cats/data.ts';
	import { setConnection } from '#lib/devices/cats/connection.ts';

	const list = live(devices);
	const id = $derived(page.params.deviceId ?? '');
	const device = $derived(list.data?.devices.find((d) => d.id === id) ?? list.data?.devices.find((d) => d.type === id));
	const facts = $derived(
		device
			? [device.productName || TYPE_LABEL[device.type](), device.ip, device.version && m.device_version({ version: device.version })]
					.filter(Boolean)
					.join(' · ')
			: ''
	);
	const title = $derived(device?.name ?? (list.data ? m.device_not_found() : list.failed ? m.device_title() : m.common_loading()));
	const g = new Gesture();
</script>

<div class="measure">
	{#if device}
		<PageHead back title={device.name}>
			{#snippet sub()}
				<span class:warn-text={!device.connected}>{device.connected ? m.device_online() : m.device_offline()}</span> · {facts}
			{/snippet}
		</PageHead>
	{:else}
		<PageHead back {title} />
	{/if}
	<Loaded value={list} empty={!device} emptyText={m.device_not_found_description()}>
		{#if device}
			<ul class="settings">
				<SettingRow
					icon="wifi"
					label={m.device_local_connection()}
					hint={m.device_local_connection_hint()}
					checked={device.connected}
					busy={g.is()}
					onchange={(on) => setConnection(g, device, on)}
				/>
			</ul>
			<!-- keyed: another device's page starts from its own state -->
			{#key device.id}
				<div class="panels">
					{#if device.type === 'feeder'}
						<FeederPanel id={device.id} {device} />
					{:else if device.type === 'fountain'}
						<FountainPanel id={device.id} />
					{:else if device.type === 'litter-box'}
						<LitterBoxPanel id={device.id} {device} />
					{:else}
						<p class="empty">{m.device_unknown_type()}</p>
					{/if}
				</div>
			{/key}
		{/if}
	</Loaded>
</div>

<style>
	.settings {
		margin-bottom: var(--s-4);
	}
	.panels {
		display: grid;
		gap: var(--s-4);
	}
</style>
