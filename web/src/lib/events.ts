// Generated from api/src/events.rs (`cargo run --manifest-path api/Cargo.toml --bin shared-ts events >
// web/src/lib/events.ts`): never edited. What GET /api/events says changed: the prefix of the
// live() keys that read it again.

export const CHANGE = 'change';
export const KEEP_ALIVE_MS = 25000;
export const PUSHED_SAFETY_MS = 60000;

/** The fixed events; a cats' device says `tuya:<id>:`. */
export const EVENTS = {
	ZIGBEE_LAMPS: 'zigbee-lamp',
	SHUTTERS: 'shutters:list',
	TEMPO: 'tempo',
	MEROSS: 'meross',
	CLIMATE: 'broadlink:climate-state',
	IR_KEYS: 'ir:recent',
	ANDROID_TV: 'androidtv',
	PHOTOS: 'photos'
} as const;

/** The prefix a pushed source's key starts with. */
export const PUSHED: readonly string[] = [...Object.values(EVENTS), 'tuya:'];
