// The remote configurator's world: the devices its pickers offer, and the routes that serve
// them (actions.ts liveSources). Reuses the lamp, shutter and plug fixtures.

import type { Scene } from '#lib/devices/scenes/api.ts';
import type { Sources } from '#lib/devices/remote/actions.ts';
import { hueLamp, zigbeeLamp } from './lamps.ts';
import { merossPlug } from './meross.ts';
import { shutter } from './shutters.ts';
import { broadlinkCode, broadlinkHost } from './broadlink.ts';

export { broadlinkCode, broadlinkHost };

export const scene = (over: Partial<Scene> = {}): Scene => ({
	id: 'nuit',
	name: 'Nuit',
	icon: 'moon',
	actions: [{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' }],
	...over
});

/** One of each: a lamp « Suspension » (zb-1), a Hue lamp « Lampe du salon » (hue-1), a shutter
 * « Salon » (s1), a plug « Radiateur » (p1), « RM4 salon », « Ventilateur », the box's
 * « SmartTube », a scene « Nuit ». */
export const remoteSources = (): Sources => ({
	lamps: [zigbeeLamp()],
	hueLamps: [hueLamp()],
	covers: [shutter()],
	plugs: [merossPlug()],
	hosts: [broadlinkHost()],
	codes: [broadlinkCode()],
	apps: [{ package: 'org.smarttube.beta', label: 'SmartTube' }],
	scenes: [scene()]
});

/** The routes liveSources() reads, serving `s` (stubApi keys). */
export const sourceRoutes = (s: Sources = remoteSources()) => ({
	'/zigbee/lamps': { success: true, lamps: s.lamps },
	'/hue-lamps': {
		success: true,
		lamps: s.hueLamps,
		total: s.hueLamps.length,
		connected: s.hueLamps.length,
		reachable: s.hueLamps.length,
		message: ''
	},
	'/matter/covers': { success: true, covers: s.covers },
	'/meross': { success: true, devices: s.plugs, total: s.plugs.length, message: '' },
	'/broadlink/discover': { success: true, devices: s.hosts, total: s.hosts.length, message: '' },
	'/broadlink/codes': { success: true, codes: s.codes, total: s.codes.length, message: '' },
	'/androidtv': {
		success: true,
		config: { host: '192.168.1.153', favoriteApps: s.apps },
		status: { configured: true, reachable: true, awake: false, paired: true }
	},
	'/scenes': { success: true, scenes: s.scenes }
});
