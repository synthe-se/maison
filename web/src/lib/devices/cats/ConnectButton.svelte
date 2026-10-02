<script lang="ts">
	// Connect or disconnect one Tuya device (the server keeps a local socket to it).
	import { m } from '#lib/paraglide/messages.js';
	import { devicesApi, type Device } from '#lib/api.ts';
	import { refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
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

<button
	class="icon-btn"
	aria-label={device.connected ? m.device_disconnect({ name: device.name }) : m.device_connect({ name: device.name })}
	aria-busy={g.is()}
	disabled={g.is()}
	onclick={toggle}
>
	<Icon name={g.is() ? 'loader-circle' : device.connected ? 'wifi-off' : 'wifi'} class={g.is() ? 'spin' : undefined} />
</button>
