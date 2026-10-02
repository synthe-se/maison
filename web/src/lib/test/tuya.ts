// Tuya device fixtures as `/api/devices` lists them, for the unit tests and the e2e house.

import type { Device } from '#lib/api.ts';

export const tuyaDevice = (over: Partial<Device> = {}): Device => ({
	id: 'f1',
	name: 'Distributeur',
	type: 'feeder',
	connected: true,
	...over
});
