// What a remote button or a scene can do: the action types the editors edit, their one label
// map, their one summary builder, and the devices offered in the pickers. The backend knows
// more types (tv_key, androidtv_key…) that the editors do not edit: they are kept as they
// are, summarised by their name, and never block a save.

import { m } from '#lib/paraglide/messages.js';
import { live } from '#lib/live.svelte.ts';
import { percent } from '#lib/i18n.svelte.ts';
import type { AndroidApp } from '#lib/devices/tv/api.ts';
import type { BroadlinkCode, BroadlinkDevice } from '#lib/devices/climate/api.ts';
import type { HueLamp, ZigbeeLamp } from '#lib/devices/lamps/api.ts';
import type { MerossPlug } from '#lib/devices/meross/api.ts';
import type { Scene } from '#lib/devices/scenes/api.ts';
import type { Shutter } from '#lib/devices/shutters/api.ts';
import { hue, zigbee } from '#lib/devices/lamps/lamp.ts';
import { covers as coverList } from '#lib/devices/shutters/data.ts';
import { plugs as plugList } from '#lib/devices/meross/data.ts';
import { blasters, codes as codeList } from '#lib/devices/climate/data.ts';
import { box as boxData } from '#lib/devices/tv/data.ts';
import { scenes as sceneList } from '#lib/devices/scenes/data.ts';
import { buildClimateCommand, parseClimateCommand } from '#lib/devices/climate/command.ts';
import { degrees, FAN_LABEL, MODE_LABEL, VANE_LABEL } from '#lib/devices/climate/labels.ts';
import { boxApps } from '#lib/devices/tv/apps.ts';
import type { IrAction, IrBinding, IrCoverCommand, IrSwitchState } from './api.ts';

export type ActionType = IrAction['action'];

/** The one label map of the action types, in the order of the picker. */
export const ACTION_TYPES: Record<ActionType, () => string> = {
	nabaztag: m.nabaztag_name,
	zigbee_power: m.remote_action_types_zigbee_power,
	zigbee_brightness: m.remote_action_types_zigbee_brightness,
	hue_power: m.remote_action_types_hue_power,
	hue_brightness: m.remote_action_types_hue_brightness,
	cover: m.remote_action_types_cover,
	broadlink_code: m.remote_action_types_broadlink_code,
	meross_power: m.remote_action_types_meross_power,
	climate_toggle: m.remote_action_types_climate_toggle,
	climate_off: m.remote_action_types_climate_off,
	climate_on: m.remote_action_types_climate_on,
	tv_power: m.remote_action_types_tv_power,
	androidtv_app: m.remote_action_types_androidtv_app,
	scene: m.remote_action_types_scene
};

/** What a scene may hold: everything but another scene (the backend refuses nesting). */
export const SCENE_TYPES = (Object.keys(ACTION_TYPES) as ActionType[]).filter((t) => t !== 'scene');

export const SWITCH_STATES: Record<IrSwitchState, () => string> = {
	toggle: m.remote_fields_toggle,
	on: m.action_turn_on,
	off: m.action_turn_off
};

/** What a `cover` action can ask, in the picker's order, worded as the shutter tile words it. */
export const COVER_COMMANDS: Record<IrCoverCommand, () => string> = {
	close: m.shutters_close_action,
	open: m.shutters_open_action,
	stop: m.common_stop,
	position: m.remote_fields_cover_position
};

export const NABAZTAG_PRESETS = ['chor taichi', 'dance 1', 'ping', 'stop'];

/** Zigbee brightness, as the backend takes it (a u8 capped at the ZCL maximum). */
export const BRIGHTNESS_MAX = 254;

/** Hue brightness, as the backend takes it: a percentage, 1 at the least. */
export const HUE_BRIGHTNESS = { min: 1, max: 100 } as const;

/** Where a new Hue brightness or shutter position starts: halfway. */
export const HALF = 50;

/** An action the configurator edits (the others are carried through untouched). */
export function isEditable(a: { action: string }): a is IrAction {
	return a.action in ACTION_TYPES;
}

/** Devices offered in the action pickers (real devices, not free text). */
export interface Sources {
	lamps: ZigbeeLamp[];
	hueLamps: HueLamp[];
	covers: Shutter[];
	plugs: MerossPlug[];
	hosts: BroadlinkDevice[];
	codes: BroadlinkCode[];
	apps: AndroidApp[];
	scenes: Scene[];
}

/**
 * The pickers' devices: the dashboard's own values (same keys, same pace: a gesture's
 * `refresh` reaches them too), the IR blaster and its codes included. Call during component
 * initialisation.
 */
export function liveSources(): { readonly current: Sources } {
	const lamps = live(zigbee.list);
	const hueLamps = live(hue.list);
	const covers = live(coverList);
	const plugs = live(plugList);
	const hosts = live(blasters);
	const codes = live(codeList);
	const box = live(boxData);
	const scenes = live(sceneList);
	return {
		get current() {
			return {
				lamps: lamps.data?.lamps ?? [],
				hueLamps: hueLamps.data?.lamps ?? [],
				covers: covers.data?.covers ?? [],
				plugs: plugs.data?.devices ?? [],
				hosts: hosts.data?.devices ?? [],
				codes: codes.data?.codes ?? [],
				apps: boxApps(box.data?.config),
				scenes: scenes.data?.scenes ?? []
			};
		}
	};
}

/** A lamp of either family (Zigbee, Hue). */
const SUMMARY_LAMP: Record<IrSwitchState, (p: { lamp: string }) => string> = {
	on: m.remote_summary_zigbee_on,
	off: m.remote_summary_zigbee_off,
	toggle: m.remote_summary_zigbee_toggle
};
const SUMMARY_MEROSS: Record<IrSwitchState, (p: { device: string }) => string> = {
	on: m.remote_summary_meross_on,
	off: m.remote_summary_meross_off,
	toggle: m.remote_summary_meross_toggle
};

const SUMMARY_TV: Record<IrSwitchState, () => string> = {
	on: m.remote_summary_tv_on,
	off: m.remote_summary_tv_off,
	toggle: m.remote_summary_tv_toggle
};

const SUMMARY_COVER: Record<Exclude<IrCoverCommand, 'position'>, (p: { cover: string }) => string> = {
	open: m.remote_summary_cover_open,
	close: m.remote_summary_cover_close,
	stop: m.remote_summary_cover_stop
};

const nameOf = (list: { id: string; name: string }[], id: string) => list.find((x) => x.id === id)?.name ?? id;
const hostOf = (s: Sources, host: string) => s.hosts.find((h) => h.host === host)?.name ?? host;

/** One line per action, with the devices' real names. */
export function summarize(a: { action: string }, s: Sources): string {
	if (!isEditable(a)) return m.remote_summary_other({ action: a.action });
	switch (a.action) {
		case 'nabaztag':
			return m.remote_summary_nabaztag({ command: a.command });
		case 'zigbee_power':
			return SUMMARY_LAMP[a.state]({ lamp: nameOf(s.lamps, a.lamp) });
		case 'zigbee_brightness':
			return m.remote_summary_zigbee_brightness({
				lamp: nameOf(s.lamps, a.lamp),
				brightness: m.remote_brightness_value({ value: a.brightness })
			});
		case 'hue_power':
			return SUMMARY_LAMP[a.state]({ lamp: nameOf(s.hueLamps, a.lamp) });
		case 'hue_brightness':
			return m.remote_summary_zigbee_brightness({ lamp: nameOf(s.hueLamps, a.lamp), brightness: percent(a.brightness / 100) });
		case 'cover': {
			const cover = nameOf(s.covers, a.cover);
			return a.command === 'position'
				? m.remote_summary_cover_position({ cover, position: m.shutters_open_percent({ percent: a.position ?? 0 }) })
				: SUMMARY_COVER[a.command]({ cover });
		}
		case 'climate_off':
			return m.remote_summary_climate_off({ host: hostOf(s, a.host) });
		case 'climate_on':
			return m.remote_summary_climate_on({ host: hostOf(s, a.host) });
		case 'broadlink_code':
			return m.remote_summary_broadlink({
				host: hostOf(s, a.host),
				code: s.codes.find((c) => c.id === a.codeId)?.name ?? a.codeId
			});
		case 'meross_power':
			return SUMMARY_MEROSS[a.state]({ device: s.plugs.find((p) => p.id === a.device)?.name ?? a.device });
		case 'climate_toggle': {
			const c = parseClimateCommand(a.onCommand);
			return m.remote_summary_climate({
				host: hostOf(s, a.host),
				settings: c
					? m.remote_summary_climate_settings({
							mode: MODE_LABEL[c.mode](),
							temperature: degrees(c.temperature),
							fan: FAN_LABEL[c.fan](),
							vane: VANE_LABEL[c.vane]()
						})
					: a.onCommand
			});
		}
		case 'tv_power':
			return SUMMARY_TV[a.state]();
		case 'androidtv_app':
			return m.remote_summary_androidtv_app({ app: s.apps.find((x) => x.package === a.package)?.label ?? a.package });
		case 'scene':
			return m.remote_summary_scene({ scene: s.scenes.find((x) => x.id === a.scene)?.name ?? a.scene });
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
		case 'hue_power':
			return { action: 'hue_power', lamp: s.hueLamps[0]?.id ?? '', state: 'toggle' };
		case 'hue_brightness':
			return { action: 'hue_brightness', lamp: s.hueLamps[0]?.id ?? '', brightness: HALF };
		case 'cover':
			return { action: 'cover', cover: s.covers[0]?.id ?? '', command: 'close' };
		case 'climate_off':
			return { action: 'climate_off', host: s.hosts[0]?.host ?? '' };
		case 'climate_on':
			return { action: 'climate_on', host: s.hosts[0]?.host ?? '' };
		case 'broadlink_code':
			return { action: 'broadlink_code', host: s.hosts[0]?.host ?? '', codeId: s.codes[0]?.id ?? '' };
		case 'meross_power':
			return { action: 'meross_power', device: s.plugs[0]?.id ?? '', state: 'toggle' };
		case 'climate_toggle':
			return {
				action: 'climate_toggle',
				host: s.hosts[0]?.host ?? '',
				onCommand: buildClimateCommand({ mode: 'cool', temperature: 16, fan: '4', vane: 'swing' })
			};
		case 'tv_power':
			return { action: 'tv_power', state: 'on', switchToBox: true };
		case 'androidtv_app':
			return { action: 'androidtv_app', package: s.apps[0]?.package ?? '' };
		case 'scene':
			return { action: 'scene', scene: s.scenes[0]?.id ?? '' };
	}
}

export function isComplete(a: { action: string }): boolean {
	if (!isEditable(a)) return true;
	switch (a.action) {
		case 'nabaztag':
			return a.command.trim().length > 0;
		case 'zigbee_power':
		case 'zigbee_brightness':
		case 'hue_power':
		case 'hue_brightness':
			return a.lamp.length > 0;
		case 'cover':
			return a.cover.length > 0 && (a.command !== 'position' || a.position !== undefined);
		case 'climate_off':
		case 'climate_on':
			return a.host.length > 0;
		case 'broadlink_code':
			return a.host.length > 0 && a.codeId.length > 0;
		case 'meross_power':
			return a.device.length > 0;
		case 'climate_toggle':
			return a.host.length > 0 && a.onCommand.length > 0;
		case 'tv_power':
			return true;
		case 'androidtv_app':
			return a.package.length > 0;
		case 'scene':
			return a.scene.length > 0;
	}
}

/** One line of what running actions answered (`/ir/test`, a scene's run): « ok: … » or
 * « failed: … », and what it says after. */
export function outcome(result: string): { ok: boolean; detail: string } {
	return { ok: !result.startsWith('failed'), detail: result.replace(/^(ok|failed):\s*/, '') };
}

/** One action of an editor's list, with a stable key for its row. */
export type Row = { id: number; action: IrAction };
let nextRow = 0;
/** A row for `action`, with its own key. */
export const toRow = (action: IrAction): Row => ({ id: nextRow++, action });
/** Rows for `actions`, each with its own key. */
export const toRows = (actions: IrAction[]): Row[] => actions.map(toRow);

/** A remote key's binding as its form holds it (the key itself aside). */
export interface BindingForm {
	label: string;
	repeat: boolean;
	actions: Row[];
}

/** The form for `binding` (an empty one without). */
export const bindingForm = (binding?: IrBinding): BindingForm => ({
	label: binding?.label ?? '',
	repeat: binding?.repeat ?? false,
	actions: toRows(binding?.actions ?? [])
});
