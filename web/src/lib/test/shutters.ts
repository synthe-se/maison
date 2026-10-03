// Shutter fixtures as the backend sends them (Matter covers), for the unit tests and the e2e house.

import type { Shutter } from '#lib/devices/shutters/api.ts';

export const shutter = (over: Partial<Shutter> = {}): Shutter => ({
	id: 's1',
	name: 'Salon',
	endpoint: 1,
	online: true,
	openPercent: 40,
	targetOpenPercent: null,
	motion: 'stopped',
	vendorId: null,
	productId: null,
	schedule: { openAtSunrise: false, closeAtSunset: false, sunriseOffsetMin: 0, sunsetOffsetMin: 0 },
	nextOpen: null,
	nextClose: null,
	skipNextOpen: false,
	skipNextClose: false,
	...over
});
