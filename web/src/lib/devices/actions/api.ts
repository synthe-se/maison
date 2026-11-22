// Actions: what a scene runs, one device order each (maison-api `actions`,
// backend/src/actions.rs).

import type { Code } from '#lib/codes.ts';
/** Force a state, or flip the current one. */
export type SwitchState = 'on' | 'off' | 'toggle';

/** What a `cover` action asks of a shutter; `position` goes with its `position` field. */
export type CoverCommand = 'open' | 'close' | 'stop' | 'position';

export type Action =
	| { action: 'nabaztag'; command: string }
	| { action: 'zigbee_power'; lamp: string; state: SwitchState }
	| { action: 'zigbee_brightness'; lamp: string; brightness: number }
	| { action: 'hue_power'; lamp: string; state: SwitchState }
	/** 1 to 100. */
	| { action: 'hue_brightness'; lamp: string; brightness: number }
	/** A Matter shutter; `position` is an open percentage (0 closed, 100 open), only with `command: "position"`. */
	| { action: 'cover'; cover: string; command: CoverCommand; position?: number }
	| { action: 'broadlink_code'; host: string; codeId: string }
	| { action: 'meross_power'; device: string; state: SwitchState }
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
	| { action: 'tv_power'; state: SwitchState; switchToBox?: boolean }
	/** An app on the Android TV box, the TV woken and switched to it first. */
	| { action: 'androidtv_app'; package: string; ensureTvOn?: boolean };

/** How one action went (maison-api `ActionResult`): the action, whether it went, and the
 * refusal's code when it did not. */
export interface ActionResult {
	action: Action;
	ok: boolean;
	code?: Code;
}

/** What a scene's run answers (maison-api `RunResponse`): one result per action, in order. */
export interface RunResponse {
	success: boolean;
	/** For logs (« 3/4 actions ok »): the app says its own words. */
	message: string;
	results: ActionResult[];
}
