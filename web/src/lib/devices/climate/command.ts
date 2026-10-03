// The Mitsubishi command grammar, the one place it is written (AGENTS.md « Climate »):
//   state-<mode>-<temp>-fan-<fan>-vane-<vane>[-wide-<wide>][-econo-on][-stopin-<minutes>]
// or `state-off`. The backend (`mitsubishi_ir.rs`) encodes it; the dashboard and the IR
// remote's « climate toggle » action both build and read it through here.

import type { BroadlinkClimateSettings } from '#lib/devices/climate/api.ts';

export const CLIMATE_MODES = ['cool', 'heat', 'dry', 'fan', 'auto'] as const;
export const CLIMATE_FANS = ['auto', '1', '2', '3', '4', 'silent'] as const;
export const CLIMATE_VANES = ['auto', 'highest', 'high', 'middle', 'low', 'lowest', 'swing'] as const;

export type ClimateMode = (typeof CLIMATE_MODES)[number];
export type ClimateFan = (typeof CLIMATE_FANS)[number];
export type ClimateVane = (typeof CLIMATE_VANES)[number];

/** Setpoint bounds the Mitsubishi protocol accepts (°C). */
export const TEMP_MIN_C = 16;
export const TEMP_MAX_C = 31;

/** The unit's sleep timer counts in 10-minute ticks. */
export const STOP_IN_STEP_MIN = 10;

export const CLIMATE_OFF = 'state-off';

export interface ClimateSettings {
	mode: ClimateMode;
	temperature: number;
	fan: ClimateFan;
	vane: ClimateVane;
	/** Horizontal vane; omitted, the unit keeps its default. */
	wide?: string;
	econo?: boolean;
	/** « Turn off in N minutes », a multiple of 10. */
	stopInMinutes?: number | null;
}

export const isClimateMode = (v: string): v is ClimateMode => (CLIMATE_MODES as readonly string[]).includes(v);
export const isClimateFan = (v: string): v is ClimateFan => (CLIMATE_FANS as readonly string[]).includes(v);
export const isClimateVane = (v: string): v is ClimateVane => (CLIMATE_VANES as readonly string[]).includes(v);

export const clampTemperature = (t: number) => Math.min(TEMP_MAX_C, Math.max(TEMP_MIN_C, Math.round(t)));

export function buildClimateCommand(s: ClimateSettings): string {
	const parts = ['state', s.mode, String(s.temperature), 'fan', s.fan, 'vane', s.vane];
	if (s.wide) parts.push('wide', s.wide);
	if (s.econo) parts.push('econo', 'on');
	if (s.stopInMinutes) parts.push('stopin', String(s.stopInMinutes));
	return parts.join('-');
}

const GRAMMAR = /^state-([a-z]+)-(\d+)-fan-([a-z0-9]+)-vane-([a-z]+)(?:-wide-([a-z]+))?(?:-econo-(on|off))?(?:-stopin-(\d+))?$/;

/**
 * The settings a command carries, or null when it is `state-off` or uses parts this module
 * does not model (absolute `stop-HH-MM`, `timer-off`, unknown tokens): edit those as raw text.
 */
export function parseClimateCommand(command: string): ClimateSettings | null {
	const g = GRAMMAR.exec(command);
	if (!g) return null;
	const [, mode, temp, fan, vane, wide, econo, stopIn] = g;
	if (!isClimateMode(mode) || !isClimateFan(fan) || !isClimateVane(vane)) return null;
	return {
		mode,
		temperature: Number(temp),
		fan,
		vane,
		wide,
		econo: econo === 'on',
		stopInMinutes: stopIn ? Number(stopIn) : null
	};
}

/** The backend's parsed last command, made safe for a form: unknown values fall back one by one. */
export function settingsFromBackend(s: BroadlinkClimateSettings, fallback: ClimateSettings): ClimateSettings {
	return {
		...fallback,
		mode: isClimateMode(s.mode) ? s.mode : fallback.mode,
		temperature: clampTemperature(s.temperature),
		fan: isClimateFan(s.fan) ? s.fan : fallback.fan,
		vane: isClimateVane(s.vane) ? s.vane : fallback.vane,
		econo: s.econo,
		stopInMinutes: s.stopInMinutes
	};
}
