// What the cat devices report, as the backend reads it (backend/src/tuya/parse.rs): each token
// once, with its words and what it means. A token the app does not know is said « Inconnu »
// and logged, never shown raw.

import { m } from '#lib/paraglide/messages.js';
import type { FountainStatus, LitterBoxStatus } from './api.ts';

/** The litter box's state (`system.state`); `satnd_by` is the device's own spelling. */
export const LITTER_STATE = {
	cleaning: m.litter_box_status_cleaning,
	cat_inside: m.litter_box_status_cat_inside,
	clumping: m.litter_box_status_clumping,
	satnd_by: m.state_standby
} as const;
export type LitterState = keyof typeof LITTER_STATE;
/** Waiting between two visits: the litter has been cleaned. */
export const STANDBY: LitterState = 'satnd_by';

/** The litter's level (`sensors.litterLevel`). */
export const LITTER_LEVEL = { full: m.litter_box_filled, half: m.litter_box_half_filled } as const;
/** Half full: it wants a refill soon. */
export const litterHalf = (s: LitterBoxStatus | undefined) => s?.sensors?.litterLevel === ('half' satisfies keyof typeof LITTER_LEVEL);

/** The fountain's water (`waterLevel`). */
export const WATER_LEVEL = { low: m.device_level_low, medium: m.device_level_medium, ok: m.fountain_water_level_ok } as const;
/** Low: it wants water. */
export const waterLow = (s: FountainStatus | undefined) => s?.waterLevel === ('low' satisfies keyof typeof WATER_LEVEL);

/** What powers the feeder (`system.poweredBy`). */
export const FEEDER_POWER = { 'AC Power': m.feeder_power_mains, Battery: m.feeder_power_battery, Unknown: m.common_unknown } as const;

const told = new Set<string>();

/** `token` in the words of `table`; one the app does not know is « Inconnu », logged once. */
export function wordOf(table: Record<string, () => string>, token: string | null | undefined): string {
	if (!token) return m.common_unknown();
	if (Object.hasOwn(table, token)) return table[token]();
	if (!told.has(token)) {
		told.add(token);
		console.warn(`a device said « ${token} », which the app does not know`);
	}
	return m.common_unknown();
}
