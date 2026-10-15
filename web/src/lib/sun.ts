// The house's sun as the wall screens' theme reads it: when it next rises and sets (the house's
// place answers it), the side it says now, and when to ask again. Pure: tested on its own.

/** When the sun next rises and sets at the house (backend `GET /matter/place`). */
export interface NextSun {
	sunriseMs: number | null;
	sunsetMs: number | null;
}

/** The side the sun says now: dark when it rises next, light when it sets next; null when it
 * says nothing (no place set). A polar night or day has only one of the two ahead. */
export function sunSide(sun: NextSun | null | undefined): 'light' | 'dark' | null {
	if (!sun || (sun.sunriseMs === null && sun.sunsetMs === null)) return null;
	if (sun.sunriseMs === null) return 'light';
	if (sun.sunsetMs === null) return 'dark';
	return sun.sunriseMs < sun.sunsetMs ? 'dark' : 'light';
}

/** How long until the sun's next event, and a little after (the answer then names the following
 * ones), for its source's pace; 0 when it says nothing. */
export function untilSun(sun: NextSun | null | undefined, now = Date.now()): number {
	const next = Math.min(sun?.sunriseMs ?? Infinity, sun?.sunsetMs ?? Infinity);
	return Number.isFinite(next) ? Math.max(1_000, next - now + 5_000) : 0;
}
