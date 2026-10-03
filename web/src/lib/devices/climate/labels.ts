// The words for the Mitsubishi settings, once: the dashboard and the IR remote's climate
// action both say them through these maps.

import { m } from '#lib/paraglide/messages.js';
import { num } from '#lib/i18n.svelte.ts';
import { isClimateMode, type ClimateFan, type ClimateMode, type ClimateVane } from './command.ts';

export const MODE_LABEL: Record<ClimateMode, () => string> = {
	cool: m.climate_modes_cool,
	heat: m.climate_modes_heat,
	dry: m.climate_modes_dry,
	fan: m.climate_fan,
	auto: m.climate_auto
};

export const FAN_LABEL: Record<ClimateFan, () => string> = {
	auto: m.climate_auto,
	'1': () => m.climate_fan_levels_level({ level: 1 }),
	'2': () => m.climate_fan_levels_level({ level: 2 }),
	'3': () => m.climate_fan_levels_level({ level: 3 }),
	'4': () => m.climate_fan_levels_level({ level: 4 }),
	silent: m.climate_fan_levels_silent
};

export const VANE_LABEL: Record<ClimateVane, () => string> = {
	auto: m.climate_auto,
	highest: m.climate_vanes_highest,
	high: m.climate_vanes_high,
	middle: m.climate_vanes_middle,
	low: m.climate_vanes_low,
	lowest: m.climate_vanes_lowest,
	swing: m.climate_vanes_swing
};

/** « 21 °C » on screen. */
export const degrees = (t: number) => m.climate_degrees({ degrees: num(t) });

/** A mode as the backend stored it: its word when known, the token otherwise. */
export const modeLabel = (mode: string): string => (isClimateMode(mode) ? MODE_LABEL[mode]() : mode);
