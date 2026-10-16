// Generated from api/src/paces.rs (`cargo run --manifest-path api/Cargo.toml --bin paces-ts >
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
