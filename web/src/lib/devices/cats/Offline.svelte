<script lang="ts">
	// Why a device's controls cannot act (it is offline), next to them, and the way out
	// (« Reconnecter »): the controls stay in place, `aria-disabled` and described by this line
	// (docs/ux.md § 2: never disabled under the fingers, the reason said).
	import { m } from '#lib/paraglide/messages.js';
	import type { Device } from '#lib/devices/cats/api.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import { setConnection } from './connection.ts';

	/** `id`: what the controls point their `aria-describedby` at. */
	let { device, id, reason }: { device: Pick<Device, 'id' | 'name'>; id: string; reason: string } = $props();
	const g = new Gesture();
</script>

<div class="offline">
	<p class="warn-text" {id}>{reason}</p>
	<button class="btn" {...pending(g.is())} onclick={() => setConnection(g, device, true)}>
		<Icon name="wifi" busy={g.is()} />{m.device_reconnect()}
	</button>
</div>

<style>
	.offline {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--s-2) var(--s-3);
	}
	.offline p {
		flex: 1 1 12rem;
		font: var(--t-secondary);
		font-weight: 600;
	}
</style>
