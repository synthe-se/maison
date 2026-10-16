// What the backend accepts, said once for the web app: no generator writes these yet
// (paces.ts and codes.ts are generated from maison-api), so each names the backend constant it
// copies. Keep them in step.

/** A name as the backend keeps it, in characters (backend people.rs `MAX_NAME`). */
export const MAX_NAME = 60;

/** A wall screen's pairing code as typed, « ABCD-EFGH » (backend screens.rs `CODE_LEN`, and its
 * dash). */
export const SCREEN_CODE_LENGTH = 9;

/** The code the TV shows to pair the box: six hexadecimal characters (backend atvremote.rs). */
export const BOX_CODE_LENGTH = 6;

/** Portions in one manual serving (backend routes/devices.rs `FEEDER_WARN_PORTIONS`). */
export const FEED_MAX_PORTIONS = 12;

/** Portions in one meal of the plan (backend tuya/feeder.rs `MAX_PORTIONS`). */
export const MEAL_MAX_PORTIONS = 10;

/** Meals in a feeder's plan (backend tuya/feeder.rs `MAX_MEALS`). */
export const MAX_MEALS = 10;

/** Actions in one scene (backend scenes.rs `MAX_ACTIONS`). */
export const MAX_ACTIONS = 20;

/** The air conditioner's model, whose learnt codes the backend looks up first (the living
 * room's unit; backend broadlink.rs `send_mitsubishi_command`). */
export const CLIMATE_MODEL = 'msz-hj5va';
