// The lamps' API: Hue over Bluetooth (backend/src/hue/) and Zigbee on the server's own
// coordinator (backend/src/zigbee/).

import { get, post, path } from '#lib/api.ts';
export interface HueLampState {
	isOn: boolean;
	brightness: number;
	temperature: number | null;
	temperatureMin: number | null;
	temperatureMax: number | null;
}

export interface HueLamp {
	id: string;
	name: string;
	address: string;
	model: string | null;
	manufacturer: string;
	firmware: string | null;
	connected: boolean;
	connecting: boolean;
	reachable: boolean;
	state: HueLampState;
	lastSeen: string | null;
}

export interface HueLampsResponse {
	success: boolean;
	lamps: HueLamp[];
	total: number;
	connected: number;
	reachable: number;
	message: string;
}

export interface HueLampStatusResponse {
	success: boolean;
	lamp?: HueLamp;
	message?: string;
	error?: string;
}

export interface HueLampActionResponse {
	success: boolean;
	state?: { isOn: boolean; brightness: number };
	message?: string;
	error?: string;
}

/** A lamp family's counts; `disabled` when the server runs without that radio. */
export interface LampStats {
	success: boolean;
	total: number;
	connected: number;
	reachable: number;
	disabled?: boolean;
	message?: string;
}

export const hueLampsApi = {
	list: () => get<HueLampsResponse>('/hue-lamps'),
	scan: () => post('/hue-lamps/scan'),
	stats: () => get<LampStats>('/hue-lamps/stats'),
	status: (id: string) => get<HueLampStatusResponse>(path`/hue-lamps/${id}`),
	power: (id: string, enabled: boolean) => post<HueLampActionResponse>(path`/hue-lamps/${id}/power`, { enabled }),
	brightness: (id: string, brightness: number) => post<HueLampActionResponse>(path`/hue-lamps/${id}/brightness`, { brightness }),
	temperature: (id: string, temperature: number) => post<HueLampActionResponse>(path`/hue-lamps/${id}/temperature`, { temperature }),
	blacklist: (id: string) => post(path`/hue-lamps/${id}/blacklist`)
};

export interface ZigbeeLampState {
	isOn: boolean;
	brightness: number;
	temperature: number | null;
	temperatureMin: number | null;
	temperatureMax: number | null;
	colorX: number | null;
	colorY: number | null;
	colorMode: number | null;
}

export interface ZigbeeLamp {
	id: string;
	name: string;
	address: string;
	friendlyName: string;
	interviewCompleted: boolean;
	model: string | null;
	manufacturer: string;
	connected: boolean;
	reachable: boolean;
	supportsBrightness: boolean;
	supportsTemperature: boolean;
	supportsColor: boolean;
	state: ZigbeeLampState;
	lastSeen: string | null;
}

export interface ZigbeeLampsResponse {
	success: boolean;
	lamps: ZigbeeLamp[];
	total: number;
	connected: number;
	reachable: number;
	message: string;
}

export interface ZigbeeLampStatusResponse {
	success: boolean;
	lamp?: ZigbeeLamp;
	message?: string;
	error?: string;
}

export interface ZigbeePairingStatus {
	active: boolean;
	remainingSeconds: number;
	permitJoinSeconds: number;
	message?: string;
}

export interface ZigbeePairingResponse {
	success: boolean;
	pairing: ZigbeePairingStatus;
	message: string;
}

export interface ZigbeeLampActionResponse {
	success: boolean;
	state?: ZigbeeLampState;
	message?: string;
	error?: string;
}

export const zigbeeLampsApi = {
	list: () => get<ZigbeeLampsResponse>('/zigbee/lamps'),
	stats: () => get<LampStats>('/zigbee/lamps/stats'),
	status: (id: string) => get<ZigbeeLampStatusResponse>(path`/zigbee/lamps/${id}`),
	pairingStatus: () => get<ZigbeePairingResponse>('/zigbee/lamps/pairing/status'),
	startPairing: () => post<ZigbeePairingResponse>('/zigbee/lamps/pairing/start'),
	stopPairing: () => post<ZigbeePairingResponse>('/zigbee/lamps/pairing/stop'),
	touchlinkScan: () => post('/zigbee/lamps/pairing/touchlink'),
	power: (id: string, enabled: boolean) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/power`, { enabled }),
	brightness: (id: string, brightness: number) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/brightness`, { brightness }),
	temperature: (id: string, temperature: number) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/temperature`, { temperature }),
	color: (id: string, x: number, y: number) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/color`, { x, y }),
	effect: (id: string, effect: string) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/effect`, { effect }),
	rename: (id: string, name: string) => post(path`/zigbee/lamps/${id}/rename`, { name })
};
