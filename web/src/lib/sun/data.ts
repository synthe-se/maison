// What the sun's theme reads, each under one key: signed out, the side alone, asked every ten
// minutes (signed in, the house's place does: devices/shutters/data.ts); and the side last said,
// kept until it no longer holds: app.html applies it before first paint, the layout while the sun
// is being asked (no flash of daylight at night).

import { PUBLIC_SUN_KEY } from '$app/env/public';
import { source } from '#lib/live.svelte.ts';
import { SUN_SIGNED_OUT_MS } from '#lib/paces.ts';
import { json, stored } from '#lib/stored.ts';
import { isSunMark, sunNow, type Side, type SunMark } from './api.ts';

export const sideNow = source('sun:side', sunNow, SUN_SIGNED_OUT_MS);

export const lastSide = stored<SunMark | null>(PUBLIC_SUN_KEY, null, json(isSunMark));

/** The side last said while it still holds, or null: what stands until the sun is asked again
 * (app.html reads it the same way before first paint). */
export function keptSide(now = Date.now()): Side | null {
	const mark = lastSide.get();
	return mark && now < mark.untilMs ? mark.side : null;
}
