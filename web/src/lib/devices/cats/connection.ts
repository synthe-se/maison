// A Tuya device's local connection (the server keeps a socket to it), switched from its page,
// the group's menu or a « Reconnecter » button: one gesture, one wording, then every Tuya view
// read again.

import { m } from '#lib/paraglide/messages.js';
import { devicesApi, type Device } from './api.ts';
import { refresh } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import type { Gesture } from '#lib/gesture.svelte.ts';
import { TUYA } from './data.ts';

/** Connects (`on`) or disconnects `d` through the caller's gesture, keyed by the device. */
export function setConnection(g: Gesture, d: Pick<Device, 'id' | 'name'>, on: boolean) {
	return g.run(
		() => (on ? devicesApi.connect(d.id) : devicesApi.disconnect(d.id)),
		async () => {
			ui.toast(on ? m.device_connection_initiated_description({ name: d.name }) : m.device_disconnected_description({ name: d.name }));
			await refresh(TUYA);
		},
		`connection-${d.id}`
	);
}
