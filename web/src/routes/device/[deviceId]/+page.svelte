<script lang="ts">
	import PageHead from '#lib/components/PageHead.svelte';
	// A cat device's page (feeder, fountain, litter box): reached from its tile, « Retour à
	// l'accueil » at the top (docs/ux/tableau-de-bord.md § 5), one column at --measure.
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import ConnectButton from '#lib/devices/cats/ConnectButton.svelte';
	import FeederControl from '#lib/devices/cats/FeederControl.svelte';
	import FountainControl from '#lib/devices/cats/FountainControl.svelte';
	import LitterBoxControl from '#lib/devices/cats/LitterBoxControl.svelte';
	import { TYPE_LABEL, devices } from '#lib/devices/cats/tuya.svelte.ts';

	const list = devices();
	const id = $derived(page.params.deviceId ?? '');
	const device = $derived(list.data?.devices.find((d) => d.id === id));
	const facts = $derived(
		device
			? [device.product_name || TYPE_LABEL[device.type](), device.ip, device.version && m.device_version({ version: device.version })].filter(Boolean).join(' · ')
			: ''
	);
</script>

<div class="measure">
	{#if list.loading}
		<PageHead back title={m.common_loading()} />
	{:else if !device}
		<PageHead back title={m.device_not_found()} />
		<p class="hint">{m.device_not_found_description()}</p>
	{:else}
		<PageHead back title={device.name}>
			{#snippet end()}<ConnectButton {device} />{/snippet}
			{#snippet sub()}
				<span class:warn={!device.connected}>{device.connected ? m.device_online() : m.device_offline()}</span> · {facts}
			{/snippet}
		</PageHead>
		<!-- keyed: another device's page starts from its own state -->
		{#key device.id}
			<div class="panels">
				{#if device.type === 'feeder'}
					<FeederControl id={device.id} />
				{:else if device.type === 'fountain'}
					<FountainControl id={device.id} />
				{:else if device.type === 'litter-box'}
					<LitterBoxControl id={device.id} />
				{:else}
					<p class="empty">{m.device_unknown_type()}</p>
				{/if}
			</div>
		{/key}
	{/if}
</div>

<style>
	.warn { color: var(--warn-text); font-weight: 600; }
	.panels { display: grid; gap: var(--s-4); }
</style>
