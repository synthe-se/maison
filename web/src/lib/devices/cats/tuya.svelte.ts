// The cats' corner (Tuya, local): what the dashboard tiles and the device pages share, once.

import { m } from '#lib/paraglide/messages.js';
import { feederApi } from './api.ts';
import { refresh } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import type { Gesture } from '#lib/gesture.svelte.ts';
import { CONFIRM, haptic } from '#lib/haptics.ts';
import type { IconName } from '#lib/components/Icon.svelte';
import type { Kind } from './data.ts';

/** Meals the feeder keeps in its own memory. */
export const MAX_MEALS = 10;

export const ICON: Record<Kind, IconName> = { feeder: 'utensils', fountain: 'droplets', 'litter-box': 'trash', unknown: 'power' };
export const TYPE_LABEL: Record<Kind, () => string> = {
	feeder: m.device_types_feeder,
	fountain: m.device_types_fountain,
	'litter-box': m.device_types_litter_box,
	unknown: m.common_unknown
};

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
