<script lang="ts">
	// Connect or disconnect one Tuya device (the server keeps a local socket to it). A pressed
	// toggle with a fixed name (APG button): its icon shows the state, a connected device never
	// shows « wifi-off ».
	import { m } from '#lib/paraglide/messages.js';
	import { devicesApi, type Device } from '#lib/api.ts';
	import { refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';

	let { device }: { device: Device } = $props();
	const g = new Gesture();

	function toggle() {
		const { id, name, connected } = device;
		return g.run(
			() => (connected ? devicesApi.disconnect(id) : devicesApi.connect(id)),
			async () => {
				ui.toast(connected ? m.device_disconnected_description({ name }) : m.device_connection_initiated_description({ name }));
				await refresh('tuya:');
			}
		);
	}
</script>

<button class="icon-btn" aria-label={m.device_connection({ name: device.name })} aria-pressed={device.connected} {...pending(g.is())} onclick={toggle}>
	<Icon name={device.connected ? 'wifi' : 'wifi-off'} busy={g.is()} />
</button>
