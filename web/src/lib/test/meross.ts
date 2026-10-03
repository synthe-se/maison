// Meross plug answers as the backend sends them (src/meross.rs): the formatted electricity
// fields carry their unit (« 230.4V », « 0.183A », « 42.4W »); `raw` is in dV, mA and mW.

import type { MerossElectricityResponse, MerossPlug, MerossPlugStatusResponse } from '#lib/devices/meross/api.ts';

export const merossPlug = (over: Partial<MerossPlug> = {}): MerossPlug => ({
	id: 'p1',
	name: 'Radiateur',
	ip: '192.168.1.40',
	isOnline: true,
	isOn: true,
	lastPing: 0,
	...over
});

export const merossElectricity = (id = 'p1'): MerossElectricityResponse => ({
	success: true,
	device: { id, name: 'Radiateur' },
	electricity: {
		voltage: '230.4V',
		current: '0.183A',
		power: '42.4W',
		raw: { channel: 0, voltage: 2304, current: 183, power: 42400 }
	},
	message: ''
});

export const merossStatus = (over: Partial<MerossPlugStatusResponse['status']> = {}, id = 'p1'): MerossPlugStatusResponse => ({
	success: true,
	device: { id, name: 'Radiateur' },
	status: {
		online: true,
		on: true,
		electricity: null,
		hardware: { type: 'mss310', version: '6.0.0', chipType: 'mt7682', uuid: 'u', mac: '48:e1:e9:00:00:01' },
		firmware: { version: '6.1.8', compileTime: '', innerIp: '192.168.1.40' },
		wifi: { signal: 72 },
		lastUpdate: 0,
		...over
	},
	message: ''
});
