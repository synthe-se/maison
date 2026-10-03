// Tuya device fixtures as `/api/devices` lists them, for the unit tests and the e2e house.

import type { Device, FeederStatus } from '#lib/devices/cats/api.ts';

export const tuyaDevice = (over: Partial<Device> = {}): Device => ({
	id: 'f1',
	name: 'Distributeur',
	type: 'feeder',
	connected: true,
	...over
});

/** What the backend reads from a feeder (backend/src/tuya/parse.rs): on mains, no fault, its
 * last meal two portions at `at` (ms). */
export const feederStatus = (at = Date.UTC(2026, 9, 2, 6), over: Partial<FeederStatus> = {}): FeederStatus => ({
	feeding: { manualFeedEnabled: true, lastFeedSize: '2 portions', lastFeedReport: 0, quickFeedAvailable: false },
	settings: { soundEnabled: true, alexaFeedEnabled: false },
	system: { poweredBy: 'AC Power', ipAddress: '192.168.1.174' },
	history: {
		raw: `R:0 C:2 T:${Math.floor(at / 1000)}`,
		parsed: { remaining: '0', count: '2', timestamp: String(Math.floor(at / 1000)), timestampReadable: '' }
	},
	...over
});
