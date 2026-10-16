// The IR remote's API: the keymap of the AirTies set-top box and its actions
// (backend/src/ir.rs).

import { get, post, put, del, path } from '#lib/api.ts';
import type { Code } from '#lib/codes.ts';
/** Force a state, or flip the current one — the remote-control default. */
export type IrSwitchState = 'on' | 'off' | 'toggle';

/** What a `cover` action asks of a shutter; `position` goes with its `position` field. */
export type IrCoverCommand = 'open' | 'close' | 'stop' | 'position';

export type IrAction =
	| { action: 'nabaztag'; command: string }
	| { action: 'zigbee_power'; lamp: string; state: IrSwitchState }
	| { action: 'zigbee_brightness'; lamp: string; brightness: number }
	| { action: 'hue_power'; lamp: string; state: IrSwitchState }
	/** 1 to 100. */
	| { action: 'hue_brightness'; lamp: string; brightness: number }
	/** A Matter shutter; `position` is an open percentage (0 closed, 100 open), only with `command: "position"`. */
	| { action: 'cover'; cover: string; command: IrCoverCommand; position?: number }
	| { action: 'broadlink_code'; host: string; codeId: string }
	| { action: 'meross_power'; device: string; state: IrSwitchState }
	| {
			action: 'climate_toggle';
			host: string;
			/** Structured Mitsubishi command, e.g. "state-cool-16-fan-4-vane-swing". */
			onCommand: string;
			model?: string;
	  }
	/** The Mitsubishi AC off (recorded as off). */
	| { action: 'climate_off'; host: string; model?: string }
	/** The Mitsubishi AC back on with the last settings it was sent, without their sleep timer. */
	| { action: 'climate_on'; host: string; model?: string }
	/** The Philips TV on or off (over IR and JointSPACE); on, it takes the box's input. */
	| { action: 'tv_power'; state: IrSwitchState; switchToBox?: boolean }
	/** An app on the Android TV box, the TV woken and switched to it first. */
	| { action: 'androidtv_app'; package: string; ensureTvOn?: boolean }
	/** A scene, by id (a remote key may run one; a scene may not contain one). */
	| { action: 'scene'; scene: string };

export interface IrBinding {
	/** Fired in order; one failing action does not stop the others. */
	actions: IrAction[];
	label?: string;
	/** Also fire on kernel autorepeat events while the button is held. */
	repeat?: boolean;
}

export interface IrKeymapResponse {
	success: boolean;
	/** Keycodes are numbers, serialized as JSON object keys (strings). */
	keymap: Record<string, IrBinding>;
}

export interface IrEvent {
	/** One more with each event since the backend started: what capture compares. */
	seq: number;
	code: number;
	/** 1 = press, 2 = autorepeat, 0 = release. */
	value: number;
	mapped: boolean;
	receivedAt: string;
}

export interface IrRecentResponse {
	success: boolean;
	/** Newest first. */
	events: IrEvent[];
}

/** How one action went (maison-api `ActionResult`): the action, whether it went, and the
 * refusal's code when it did not. */
export interface ActionResult {
	action: IrAction;
	ok: boolean;
	code?: Code;
}

/** What running actions answers (`/ir/test`, a scene's run; maison-api `KeyResponse`): one
 * result per action, in order. */
export interface KeyResponse {
	success: boolean;
	/** For logs (« 3/4 actions ok »): the app says its own words. */
	message: string;
	results: ActionResult[];
}

export const irApi = {
	keymap: () => get<IrKeymapResponse>('/ir/keymap'),
	setBinding: (code: number, binding: IrBinding) => put(path`/ir/keymap/${code}`, binding),
	removeBinding: (code: number) => del(path`/ir/keymap/${code}`),
	recent: () => get<IrRecentResponse>('/ir/recent'),
	test: (actions: IrAction[]) => post<KeyResponse>('/ir/test', { actions })
};
