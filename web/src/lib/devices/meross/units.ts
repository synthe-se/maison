// Electricity in words, once: the plug tile, the plug page. The electricity endpoint's
// formatted fields already carry their unit (« 42.0W »): numbers come from its raw reading,
// converted by `reading` alone. Daily energy is in Wh.

import { m } from '#lib/paraglide/messages.js';
import { num } from '#lib/i18n.svelte.ts';

/** « 42 W » (one decimal on the plug's page, none on a tile). */
export const watts = (w: number, digits = 0) => m.meross_watts({ power: num(w, digits) });
export const volts = (v: number) => m.meross_volts({ voltage: num(v, 1) });
/** The plug reports amperes; a small load reads better in milliamperes. */
export const milliamps = (a: number) => m.meross_milliamps({ current: num(a * 1000) });
export const kwh = (kwh: number) => m.meross_kwh({ energy: num(kwh, 3) });
export const kwhFromWh = (wh: number) => kwh(wh / 1000);

/** The plug's raw reading in base units: it reports tenths of a volt, milliamperes and
 * milliwatts (backend/src/meross.rs, `format_*`). */
export const reading = (raw: { voltage: number; current: number; power: number }) => ({
	volts: raw.voltage / 10,
	amps: raw.current / 1000,
	watts: raw.power / 1000
});

/** The live power of a plug, as a tile says it. */
export const livePower = (e: { raw: { voltage: number; current: number; power: number } }) => watts(reading(e.raw).watts);
