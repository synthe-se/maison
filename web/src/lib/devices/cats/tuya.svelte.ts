// The cats' corner (Tuya, local): what the dashboard tiles and the device pages share, once.

import { m } from '#lib/paraglide/messages.js';
import { devicesApi, feederApi, fountainApi, litterBoxApi, type Device } from '#lib/api.ts';
import { live, refresh } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import type { Gesture } from '#lib/gesture.svelte.ts';
import { clock } from '#lib/i18n.svelte.ts';
import { CONFIRM, haptic } from '#lib/haptics.ts';
import type { IconName } from '#lib/components/Icon.svelte';

export type Kind = Device['type'];
export type StatusKind = Exclude<Kind, 'unknown'>;

/** Poll intervals, as the React app had them (docs/ux/tableau-de-bord.md § 4: 10 to 15 s for Tuya). */
export const POLL = { devices: 10_000, feeder: 15_000, fountain: 10_000, 'litter-box': 15_000 } as const;

/** Meals the feeder keeps in its own memory. */
export const MAX_MEALS = 10;

export const ICON: Record<Kind, IconName> = { feeder: 'utensils', fountain: 'droplets', 'litter-box': 'trash', unknown: 'power' };
export const TYPE_LABEL: Record<Kind, () => string> = {
	feeder: m.device_types_feeder,
	fountain: m.device_types_fountain,
	'litter-box': m.device_types_litter_box,
	unknown: m.common_unknown
};

const STATUS = { feeder: feederApi.status, fountain: fountainApi.status, 'litter-box': litterBoxApi.status } as const;

/** Every Tuya device, shared by the dashboard group and the device pages. */
export const devices = () => live('tuya:devices', devicesApi.list, POLL.devices);

/** One device's parsed status, polled while its page is open. */
export function status<T>(kind: StatusKind, id: string) {
	return live(`tuya:${id}:status`, async () => (await STATUS[kind](id)).parsed_status as T | undefined, POLL[kind]);
}

/** « 08:00 » as the device writes it → the reader's format. */
export function clockOf(hhmm: string): string {
	const [h, min] = hhmm.split(':').map(Number);
	return clock(new Date(2000, 0, 1, h, min));
}

/** A detail page's follow-up to a gesture: read the device again (`prefix` of the live keys), then say `said` if given. */
export function reread(prefix: string, said?: string) {
	return async () => {
		await refresh(prefix);
		if (said) ui.say(said);
	};
}

/** When each feeder last served from this screen: the tile and the page say it (§ 6). */
export const served = $state<Record<string, number>>({});

/** Serves `count` portions through the caller's gesture (its control shows `key` in flight). */
export function feed(g: Gesture, id: string, count: number, key?: string) {
	haptic(CONFIRM);
	return g.run(
		() => feederApi.feed(id, count),
		() => {
			served[id] = Date.now();
			ui.say(m.feeder_portions_distributed({ count }));
		},
		key
	);
}
