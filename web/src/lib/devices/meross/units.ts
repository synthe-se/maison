// Electricity in words, once: the plug tile, the plug page. The electricity endpoint's
// formatted fields already carry their unit (« 42.0W »): numbers come from its raw reading,
// converted by `reading` alone. Daily energy is in Wh.

import { m } from '#lib/paraglide/messages.js';
import { num } from '#lib/i18n.svelte.ts';
import { costPerHour, perHour, redPeak, type PriceNow } from '#lib/devices/tempo/price.ts';

/** « 42 W » (one decimal on the plug's page, none on a tile). */
export const watts = (w: number, digits = 0) => m.meross_watts({ power: num(w, digits) });
export const volts = (v: number) => m.meross_volts({ voltage: num(v, 1) });
/** The plug reports amperes; a small load reads better in milliamperes. */
export const milliamps = (a: number) => m.meross_milliamps({ current: num(a * 1000) });
export const kwh = (energy: number) => m.meross_kwh({ energy: num(energy, 3) });
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

/** « 42 W · 0,7 c€/h »: the live power and what it costs at the price in force (none
 * known: the power alone). */
export const powerAndCost = (w: number, eurPerKwh: number | undefined, digits = 0) =>
	eurPerKwh === undefined || w <= 0
		? watts(w, digits)
		: m.meross_power_cost({ power: watts(w, digits), cost: perHour(costPerHour(w, eurPerKwh)) });

/** Above this a plug draws enough to matter during red peak hours (standby leaks do not). */
export const NUDGE_WATTS = 5;

/** What a plug on and drawing power costs per hour during a red day's peak hours (« 0,76 €/h »);
 * undefined at any other time, or when it draws next to nothing. */
export function redPeakCost(on: boolean, w: number | undefined, price: PriceNow | null): string | undefined {
	return on && w !== undefined && w >= NUDGE_WATTS && price && redPeak(price) ? perHour(costPerHour(w, price.price)) : undefined;
}
