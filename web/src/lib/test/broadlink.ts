// The IR blaster as the backend sends it (discovery, learned codes), for the unit tests and the
// e2e house.

import type { BroadlinkCode, BroadlinkDevice } from '#lib/devices/climate/api.ts';

export const broadlinkHost = (over: Partial<BroadlinkDevice> = {}): BroadlinkDevice => ({
	// TEST-NET (RFC 5737): no device anywhere, so an e2e run never reaches the real blaster
	host: '192.0.2.60',
	mac: 'e8:16:56:00:00:01',
	modelCode: 0x649b,
	friendlyModel: 'RM4 pro',
	friendlyType: 'RM4',
	name: 'RM4 salon',
	isLocked: false,
	kind: 'rm4',
	supportsLearning: true,
	...over
});

export const broadlinkCode = (over: Partial<BroadlinkCode> = {}): BroadlinkCode => ({
	id: 'fan-on',
	name: 'Ventilateur',
	brand: null,
	model: null,
	command: 'on',
	packetBase64: '',
	packetLength: 0,
	tags: [],
	createdAt: '',
	updatedAt: '',
	...over
});
