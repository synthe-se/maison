// What a remote button can do: the action types the configurator edits, their one label map,
// their one summary builder, and the devices offered in the pickers. The backend knows more
// types (tv_*, androidtv_*) that the configurator does not edit: they are kept as they are,
// summarised by their name, and never block a save.

import { m } from '#lib/paraglide/messages.js';
import {
	broadlinkApi,
	merossApi,
	zigbeeLampsApi,
	type BroadlinkCode,
	type BroadlinkDevice,
	type IrAction,
	type IrSwitchState,
	type MerossPlug,
	type ZigbeeLamp
} from '#lib/api.ts';
import { live } from '#lib/live.svelte.ts';
import { LIST_EVERY, zigbee } from '#lib/devices/lamps/lamp.ts';
import { LIST_EVERY_MS, LIST_KEY } from '#lib/devices/meross/keys.ts';
import { buildClimateCommand, parseClimateCommand } from '#lib/devices/climate/command.ts';
import { degrees, FAN_LABEL, MODE_LABEL, VANE_LABEL } from '#lib/devices/climate/labels.ts';

export type ActionType = IrAction['action'];

/** The one label map of the action types, in the order of the picker. */
export const ACTION_TYPES: Record<ActionType, () => string> = {
	nabaztag: m.nabaztag_name,
	zigbee_power: m.remote_action_types_zigbee_power,
	zigbee_brightness: m.remote_action_types_zigbee_brightness,
	broadlink_code: m.remote_action_types_broadlink_code,
	meross_power: m.remote_action_types_meross_power,
	climate_toggle: m.remote_action_types_climate_toggle
};

export const SWITCH_STATES: Record<IrSwitchState, () => string> = {
	toggle: m.remote_fields_toggle,
	on: m.action_turn_on,
	off: m.action_turn_off
};

export const NABAZTAG_PRESETS = ['chor taichi', 'dance 1', 'ping', 'stop'];

/** Zigbee brightness, as the backend takes it (a u8 capped at the ZCL maximum). */
export const BRIGHTNESS_MAX = 254;

/** An action the configurator edits (the others are carried through untouched). */
export function isEditable(a: { action: string }): a is IrAction {
	return a.action in ACTION_TYPES;
}

/** Devices offered in the action pickers (real devices, not free text). */
export interface Sources {
	lamps: ZigbeeLamp[];
	plugs: MerossPlug[];
	hosts: BroadlinkDevice[];
	codes: BroadlinkCode[];
}

/**
 * The pickers' devices. The lamps and plugs are the dashboard's own lists (same keys, same
 * pace: a gesture's `refresh` reaches them too); the IR blaster and its codes are asked once.
 * Call during component initialisation.
 */
export function liveSources(): { readonly current: Sources } {
	const lamps = live(zigbee.keys.list, zigbeeLampsApi.list, LIST_EVERY);
	const plugs = live(LIST_KEY, merossApi.list, LIST_EVERY_MS);
	const hosts = live('remote-src-hosts', () => broadlinkApi.discover());
	const codes = live('remote-src-codes', broadlinkApi.listCodes);
	return {
		get current() {
			return {
				lamps: lamps.data?.lamps ?? [],
				plugs: plugs.data?.devices ?? [],
				hosts: hosts.data?.devices ?? [],
				codes: codes.data?.codes ?? []
			};
		}
	};
}

const SUMMARY_ZIGBEE: Record<IrSwitchState, (p: { lamp: string }) => string> = {
	on: m.remote_summary_zigbee_on,
	off: m.remote_summary_zigbee_off,
	toggle: m.remote_summary_zigbee_toggle
};
const SUMMARY_MEROSS: Record<IrSwitchState, (p: { device: string }) => string> = {
	on: m.remote_summary_meross_on,
	off: m.remote_summary_meross_off,
	toggle: m.remote_summary_meross_toggle
};

/** One line per action, with the devices' real names. */
export function summarize(a: { action: string }, s: Sources): string {
	if (!isEditable(a)) return m.remote_summary_other({ action: a.action });
	switch (a.action) {
		case 'nabaztag':
			return m.remote_summary_nabaztag({ command: a.command });
		case 'zigbee_power':
			return SUMMARY_ZIGBEE[a.state]({ lamp: s.lamps.find((l) => l.id === a.lamp)?.name ?? a.lamp });
		case 'zigbee_brightness':
			return m.remote_summary_zigbee_brightness({
				lamp: s.lamps.find((l) => l.id === a.lamp)?.name ?? a.lamp,
				brightness: m.remote_brightness_value({ value: a.brightness })
			});
		case 'broadlink_code':
			return m.remote_summary_broadlink({
				host: s.hosts.find((h) => h.host === a.host)?.name ?? a.host,
				code: s.codes.find((c) => c.id === a.code_id)?.name ?? a.code_id
			});
		case 'meross_power':
			return SUMMARY_MEROSS[a.state]({ device: s.plugs.find((p) => p.id === a.device)?.name ?? a.device });
		case 'climate_toggle': {
			const c = parseClimateCommand(a.on_command);
			return m.remote_summary_climate({
				host: s.hosts.find((h) => h.host === a.host)?.name ?? a.host,
				settings: c
					? m.remote_summary_climate_settings({
							mode: MODE_LABEL[c.mode](),
							temperature: degrees(c.temperature),
							fan: FAN_LABEL[c.fan](),
							vane: VANE_LABEL[c.vane]()
						})
					: a.on_command
			});
		}
	}
}

/** A new action of a type, on the first device offered. */
export function defaultAction(type: ActionType, s: Sources): IrAction {
	switch (type) {
		case 'nabaztag':
			return { action: 'nabaztag', command: '' };
		case 'zigbee_power':
			return { action: 'zigbee_power', lamp: s.lamps[0]?.id ?? '', state: 'toggle' };
		case 'zigbee_brightness':
			return { action: 'zigbee_brightness', lamp: s.lamps[0]?.id ?? '', brightness: 127 };
		case 'broadlink_code':
			return { action: 'broadlink_code', host: s.hosts[0]?.host ?? '', code_id: s.codes[0]?.id ?? '' };
		case 'meross_power':
			return { action: 'meross_power', device: s.plugs[0]?.id ?? '', state: 'toggle' };
		case 'climate_toggle':
			return {
				action: 'climate_toggle',
				host: s.hosts[0]?.host ?? '',
				on_command: buildClimateCommand({ mode: 'cool', temperature: 16, fan: '4', vane: 'swing' })
			};
	}
}

export function isComplete(a: { action: string }): boolean {
	if (!isEditable(a)) return true;
	switch (a.action) {
		case 'nabaztag':
			return a.command.trim().length > 0;
		case 'zigbee_power':
		case 'zigbee_brightness':
			return a.lamp.length > 0;
		case 'broadlink_code':
			return a.host.length > 0 && a.code_id.length > 0;
		case 'meross_power':
			return a.device.length > 0;
		case 'climate_toggle':
			return a.host.length > 0 && a.on_command.length > 0;
	}
}
