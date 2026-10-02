// Lamp fixtures as the backend sends them (Hue over Bluetooth, Zigbee), for the lamp tests.

import type { HueLamp, ZigbeeLamp } from '#lib/api.ts';

export function hueLamp(over: Partial<Omit<HueLamp, 'state'>> & { state?: Partial<HueLamp['state']> } = {}): HueLamp {
	const { state, ...rest } = over;
	return {
		id: 'hue-1',
		name: 'Lampe du salon',
		address: 'AA:BB:CC:DD:EE:FF',
		model: 'LCA001',
		manufacturer: 'Signify',
		firmware: '1.104.2',
		connected: true,
		connecting: false,
		reachable: true,
		lastSeen: null,
		...rest,
		state: { isOn: true, brightness: 80, temperature: 50, temperatureMin: 0, temperatureMax: 100, ...state }
	};
}

export function zigbeeLamp(over: Partial<Omit<ZigbeeLamp, 'state'>> & { state?: Partial<ZigbeeLamp['state']> } = {}): ZigbeeLamp {
	const { state, ...rest } = over;
	return {
		id: 'zb-1',
		name: 'Suspension',
		address: '00:17:88:01:0b:2c:3d:4e',
		friendlyName: '0x0017880104b2c3d4',
		linkQuality: 120,
		interviewCompleted: true,
		model: 'LCT015',
		manufacturer: 'Philips',
		firmware: '1.93.11',
		connected: true,
		reachable: true,
		supportsBrightness: true,
		supportsTemperature: true,
		supportsColor: true,
		lastSeen: null,
		...rest,
		state: { isOn: true, brightness: 60, temperature: 40, temperatureMin: 0, temperatureMax: 100, colorX: 0.3, colorY: 0.3, colorMode: 2, ...state }
	};
}
