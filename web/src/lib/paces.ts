// Generated from api/src/paces.rs (`cargo run --manifest-path api/Cargo.toml --bin shared-ts paces >
// web/src/lib/paces.ts`): never edited. How often each value is read again, in milliseconds,
// said once for the web app and the wall frame.

/**
 * Tempo: RTE publishes once a day (tomorrow around 10:40); the server asks it again every
 * quarter of an hour until then: half an hour is plenty (`/tempo`, `/tempo/forecast`).
 */
export const TEMPO_MS = 1800000;

/** The lamps' lists, Hue and Zigbee (as the React app did). */
export const LAMP_LIST_MS = 5000;

/** One lamp's page. */
export const LAMP_DETAIL_MS = 3000;

/** The shutters while a motor runs: the position follows the travel. */
export const SHUTTERS_MOVING_MS = 1000;

/** The shutters at rest. */
export const SHUTTERS_AT_REST_MS = 10000;

/** The plugs' list (docs/ux.md § 4: 5 s for plugs). */
export const MEROSS_LIST_MS = 5000;

/** One plug's status, on its page. */
export const MEROSS_STATUS_MS = 3000;

/** A plug's live power. */
export const MEROSS_ELECTRICITY_MS = 5000;

/** A plug's daily totals: they move slowly. */
export const MEROSS_CONSUMPTION_MS = 30000;

/**
 * When the sun says nothing (no place set yet), how soon it is asked again: a place set
 * later is followed without a reload.
 */
export const NO_SUN_MS = 3600000;

/**
 * The sun is read again this long after its next event: the answer then names the
 * following ones.
 */
export const SUN_AFTER_MS = 5000;

/** Never sooner than this, an event already past. */
export const SUN_MIN_MS = 1000;

/**
 * Signed out (a wall screen waiting to be paired), the sun's side alone, this often: it
 * knows no hours.
 */
export const SUN_SIGNED_OUT_MS = 600000;

/** Never later than this, whatever the server says of the sun. */
export const SUN_MAX_MS = 86400000;

/** The cats' devices (Tuya): their list. */
export const TUYA_DEVICES_MS = 10000;

/** The feeder's status. */
export const FEEDER_MS = 15000;

/** The fountain's status. */
export const FOUNTAIN_MS = 10000;

/** The litter box's status. */
export const LITTER_BOX_MS = 15000;

/** The rabbit's status. */
export const NABAZTAG_MS = 120000;

/** While the IR blaster is searched for, how often its list is read again. */
export const BROADLINK_SEARCH_MS = 4000;

/** How long a search for the IR blaster lasts. */
export const BROADLINK_SEARCH_FOR_MS = 120000;

/** The last climate order (only Maison changes it). */
export const CLIMATE_STATE_MS = 60000;

/** The remote's recent keys, while a key is being captured. */
export const IR_CAPTURE_MS = 1000;

/** The TV and the box: as slowly as the TV allows (JointSPACE dies of being asked). */
export const TV_MS = 60000;

/** The Zigbee network's pairing window while it is open (its countdown), and closed. */
export const ZIGBEE_PAIRING_OPEN_MS = 1000;

export const ZIGBEE_PAIRING_CLOSED_MS = 10000;

/** A value read late this many times its pace is said to be old (a count, not a time). */
export const STALE_CADENCES = 3;

/**
 * The scenes change only from their editor, which reads them again (the web app never
 * polls them); a wall screen has no editor: it reads them again this often.
 */
export const SCENES_MS = 3600000;

/** A gesture with no answer after this says it is slow (« Allumage… »). */
export const COMMAND_SLOW_MS = 1000;

/** How long a lamp may take to answer before « Pas de réponse ». */
export const LAMP_LIMIT_MS = 3000;

/** How long a plug may take. */
export const PLUG_LIMIT_MS = 5000;

/** How long the TV may take (its IR power and the box's wake). */
export const TV_LIMIT_MS = 30000;

/** How long any other device may take. */
export const DEVICE_LIMIT_MS = 10000;

/** A live slider sends at most this often while it moves. */
export const SLIDER_THROTTLE_MS = 300;

/** A slider let go holds its value this long against an older reading. */
export const SLIDER_HOLD_MS = 3000;

/** How long a toast stays. */
export const TOAST_MS = 5000;

/** How long a toast with an action stays (time to reach it). */
export const ACTION_TOAST_MS = 10000;

/**
 * A warning stays until dismissed on a page; on a wall screen, which nobody dismisses, this
 * long.
 */
export const WALL_WARNING_MS = 10000;
