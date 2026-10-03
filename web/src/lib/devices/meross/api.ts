// The Meross plugs' API (backend/src/meross.rs).

import { get, post, path, type DeviceRef } from '#lib/api.ts';
export interface MerossPlug {
	id: string;
	name: string;
	ip: string;
	isOnline: boolean;
	isOn: boolean;
	/** When the plug last answered (ms since the epoch); 0: never. */
	lastPing: number;
}

export interface MerossPlugsResponse {
	success: boolean;
	devices: MerossPlug[];
	total: number;
	message: string;
}

export interface MerossPlugStatus {
	online: boolean;
	on: boolean;
	electricity: { voltage: number; current: number; power: number } | null;
	hardware: { type: string; version: string; chipType: string; uuid: string; mac: string } | null;
	firmware: { version: string; compileTime: string; innerIp: string } | null;
	wifi: { signal: number | null };
	lastUpdate: number;
}

export interface MerossPlugStatusResponse {
	success: boolean;
	device: DeviceRef;
	status: MerossPlugStatus;
	message: string;
}

export interface MerossElectricityResponse {
	success: boolean;
	device: DeviceRef;
	electricity: {
		voltage: string;
		current: string;
		power: string;
		raw: {
			channel: number;
			current: number;
			voltage: number;
			power: number;
			config?: { voltageRatio: number; electricityRatio: number };
		};
	};
	message: string;
}

export interface MerossToggleResponse {
	success: boolean;
	device: DeviceRef;
	on: boolean;
	message: string;
}

export interface MerossConsumptionResponse {
	success: boolean;
	device: DeviceRef;
	consumption: Array<{ date: string; time: number; value: number }>;
	summary: { days: number; totalWh: number; totalKwh: number };
	message: string;
}

export const merossApi = {
	list: () => get<MerossPlugsResponse>('/meross'),
	status: (id: string) => get<MerossPlugStatusResponse>(path`/meross/${id}/status`),
	electricity: (id: string) => get<MerossElectricityResponse>(path`/meross/${id}/electricity`),
	toggle: (id: string, on: boolean) => post<MerossToggleResponse>(path`/meross/${id}/toggle`, { on }),
	consumption: (id: string) => get<MerossConsumptionResponse>(path`/meross/${id}/consumption`),
	dnd: (id: string, enabled: boolean) => post(path`/meross/${id}/dnd`, { enabled })
};
