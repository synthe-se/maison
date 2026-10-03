// The cats' corner (Tuya, local): the feeder, the fountain, the litter box, as the backend
// reads them (backend/src/tuya/).

import { get, post, path, type DeviceRef } from '#lib/api.ts';
export interface Device {
	id: string;
	name: string;
	type: 'feeder' | 'litter-box' | 'fountain' | 'unknown';
	productName?: string;
	model?: string;
	ip?: string;
	version?: string;
	connected: boolean;
	lastData?: unknown;
	parsedData?: unknown;
}

export interface DevicesResponse {
	success: boolean;
	devices: Device[];
	total: number;
	message: string;
}

export interface DeviceStatusResponse<T = unknown> {
	success: boolean;
	device: DeviceRef & { type: string; connected: boolean };
	parsedStatus: T;
	rawDps?: unknown;
	message: string;
}

/** What the backend reads from the feeder (backend/src/tuya/parse.rs, `parse_feeder_status`).
 * `poweredBy`: « AC Power », « Battery », « Mode <n> » or « Unknown ». */
export interface FeederStatus {
	feeding?: { manualFeedEnabled?: boolean; lastFeedSize?: string; lastFeedReport?: number; quickFeedAvailable?: boolean };
	settings?: { soundEnabled?: boolean; alexaFeedEnabled?: boolean };
	system?: { faultStatus?: boolean; poweredBy?: string; ipAddress?: string };
	/** The last meal's report (`R:<left> C:<portions> T:<unix seconds>`), each field read; null
	 * until the feeder sent one. */
	history?: {
		raw: string;
		parsed: { remaining: string; count: string | null; timestamp: string | null; timestampReadable: string };
	} | null;
}

export interface FountainStatus {
	power?: boolean;
	uv?: boolean;
	/** Seconds of UV left; > 0 means the lamp is on. */
	uvRuntime?: number;
	/** 1 or 2. */
	ecoMode?: number;
	waterLevel?: string;
	/** Minutes. */
	filterLife?: number;
	pumpTime?: number;
	waterTime?: number;
}

export interface LitterBoxSettings {
	cleanDelay?: number;
	sleepMode?: { enabled?: boolean; startTime?: string; endTime?: string };
	preferences?: {
		childLock?: boolean;
		kittenMode?: boolean;
		lighting?: boolean;
		promptSound?: boolean;
		automaticHoming?: boolean;
	};
	actions?: { resetSandLevel?: boolean; resetFactorySettings?: boolean };
}

export type LitterBoxPreference = keyof NonNullable<LitterBoxSettings['preferences']>;

export interface LitterBoxStatus {
	cleanDelay?: { seconds?: number };
	sleepMode?: { enabled?: boolean; startTimeFormatted?: string; endTimeFormatted?: string };
	sensors?: { litterLevel?: string; faultAlarm?: number };
	system?: { state?: string; maintenanceRequired?: boolean };
	settings?: Partial<Record<LitterBoxPreference, boolean>>;
}

export interface MealPlanEntry {
	daysOfWeek: string[];
	time: string;
	portion: number;
	status: 'Enabled' | 'Disabled';
}

export interface MealPlanResponse {
	success: boolean;
	device: DeviceRef;
	decoded: MealPlanEntry[] | null;
	mealPlan: string | null;
	message: string;
}

export const devicesApi = {
	list: () => get<DevicesResponse>('/devices'),
	connect: (id: string) => post(path`/devices/${id}/connect`),
	connectAll: () => post('/devices/connect'),
	disconnect: (id: string) => post(path`/devices/${id}/disconnect`),
	disconnectAll: () => post('/devices/disconnect')
};

export const feederApi = {
	status: (id: string) => get<DeviceStatusResponse<FeederStatus>>(path`/devices/${id}/feeder/status`),
	feed: (id: string, portion = 1) => post(path`/devices/${id}/feeder/feed`, { portion }),
	getMealPlan: (id: string) => get<MealPlanResponse>(path`/devices/${id}/feeder/meal-plan`),
	setMealPlan: (id: string, mealPlan: MealPlanEntry[]) => post(path`/devices/${id}/feeder/meal-plan`, { mealPlan })
};

export const fountainApi = {
	status: (id: string) => get<DeviceStatusResponse<FountainStatus>>(path`/devices/${id}/fountain/status`),
	power: (id: string, enabled: boolean) => post(path`/devices/${id}/fountain/power`, { enabled }),
	resetWater: (id: string) => post(path`/devices/${id}/fountain/reset/water`),
	resetFilter: (id: string) => post(path`/devices/${id}/fountain/reset/filter`),
	resetPump: (id: string) => post(path`/devices/${id}/fountain/reset/pump`),
	setUV: (id: string, enabled: boolean) => post(path`/devices/${id}/fountain/uv`, { enabled }),
	/** 1 = mode 1, 2 = mode 2. */
	setEcoMode: (id: string, mode: number) => post(path`/devices/${id}/fountain/eco-mode`, { mode })
};

export const litterBoxApi = {
	status: (id: string) => get<DeviceStatusResponse<LitterBoxStatus>>(path`/devices/${id}/litter-box/status`),
	clean: (id: string) => post(path`/devices/${id}/litter-box/clean`),
	settings: (id: string, settings: LitterBoxSettings) => post(path`/devices/${id}/litter-box/settings`, settings)
};
