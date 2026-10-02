// Shutter fixtures as the backend sends them (Matter covers), for the unit tests and the e2e house.

import type { Shutter } from '#lib/api.ts';

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
	...over
});
