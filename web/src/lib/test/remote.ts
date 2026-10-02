// The remote configurator's world: the devices its pickers offer, and the routes that serve
// them (actions.ts liveSources). Reuses the lamp and plug fixtures.

import type { BroadlinkCode, BroadlinkDevice } from '#lib/api.ts';
import type { Sources } from '#lib/devices/remote/actions.ts';
import { zigbeeLamp } from './lamps.ts';
import { merossPlug } from './meross.ts';

export const broadlinkHost = (over: Partial<BroadlinkDevice> = {}): BroadlinkDevice => ({
	host: '192.168.1.60',
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

/** One of each: a lamp « Suspension » (zb-1), a plug « Radiateur » (p1), « RM4 salon », « Ventilateur ». */
export const remoteSources = (): Sources => ({
	lamps: [zigbeeLamp()],
	plugs: [merossPlug()],
	hosts: [broadlinkHost()],
	codes: [broadlinkCode()]
});

/** The routes liveSources() reads, serving `s` (stubApi keys). */
export const sourceRoutes = (s: Sources = remoteSources()) => ({
	'/zigbee/lamps': { success: true, lamps: s.lamps },
	'/meross': { success: true, devices: s.plugs, total: s.plugs.length, message: '' },
	'/broadlink/discover': { success: true, devices: s.hosts, total: s.hosts.length, message: '' },
	'/broadlink/codes': { success: true, codes: s.codes, total: s.codes.length, message: '' }
});
