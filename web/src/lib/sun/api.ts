// The house's sun as the theme reads it (backend sun.rs decides the side): when it next rises
// and sets, and the side that says now, for those signed in (the house's place); the side alone
// for a page signed out (a wall screen waiting to be paired). Pure: e2e's types read it too.

import { get } from '#lib/api.ts';
import { NO_SUN_MS, SUN_AFTER_MS, SUN_MAX_MS, SUN_MIN_MS } from '#lib/paces.ts';

/** Light from sunrise to sunset at the house, dark the rest. */
export type Side = 'light' | 'dark';

/** When the sun next rises and sets at the house, and the side now (backend `GET /matter/place`). */
export interface NextSun {
	sunriseMs: number | null;
	sunsetMs: number | null;
	side: Side | null;
}

/** The side last said, and until when it holds: what app.html applies before first paint. */
export interface SunMark {
	side: Side;
	untilMs: number;
}

export const isSunMark = (v: unknown): v is SunMark => {
	const m = v as SunMark | null;
	return !!m && (m.side === 'light' || m.side === 'dark') && typeof m.untilMs === 'number';
};

/** Signed out, the side alone (backend `GET /sun`). */
export const sunNow = () => get<{ side: Side | null }>('/sun');

/** How long until the sun's next event, and a little after (the answer then names the following
 * ones), for its source's pace; an hour when it says nothing (the paces of maison_api::paces
 * `until_sun`). */
export function untilSun(sun: NextSun | null | undefined, now = Date.now()): number {
	const next = Math.min(sun?.sunriseMs ?? Infinity, sun?.sunsetMs ?? Infinity);
	return Number.isFinite(next) ? Math.min(SUN_MAX_MS, Math.max(SUN_MIN_MS, next - now + SUN_AFTER_MS)) : NO_SUN_MS;
}
