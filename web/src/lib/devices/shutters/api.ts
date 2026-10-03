// The Matter shutters' API (Sonoff Orb-RBS, backend/src/matter/).

import { get, post, put, patch, del, path } from '#lib/api.ts';
// Positions are "how open": 100 = fully open, 0 = closed; null until the switch is calibrated.

export type ShutterMotion = 'stopped' | 'opening' | 'closing';

export interface Shutter {
	id: string;
	name: string;
	endpoint: number;
	online: boolean;
	openPercent: number | null;
	targetOpenPercent: number | null;
	motion: ShutterMotion | null;
	vendorId: number | null;
	productId: number | null;
	schedule: SunSchedule;
	/** When the sun schedule will next open / close it (ISO, UTC). */
	nextOpen: string | null;
	nextClose: string | null;
	/** The next scheduled opening / closing will be skipped (once). */
	skipNextOpen: boolean;
	skipNextClose: boolean;
	error?: string;
}

/** Follow the sun: open at sunrise, close at sunset, each shifted by minutes (± 180). */
export interface SunSchedule {
	openAtSunrise: boolean;
	closeAtSunset: boolean;
	sunriseOffsetMin: number;
	sunsetOffsetMin: number;
}

/** Where the house is (for the sun schedule). */
export interface Place {
	name: string;
	latitude: number;
	longitude: number;
}

export interface ShuttersResponse {
	success: boolean;
	covers: Shutter[];
}

export interface ShutterResponse {
	success: boolean;
	cover: Shutter;
}

export const shuttersApi = {
	list: () => get<ShuttersResponse>('/matter/covers'),
	commission: (code: string, name: string) => post<ShutterResponse>('/matter/commission', { code, name }),
	open: (id: string) => post<ShutterResponse>(path`/matter/covers/${id}/open`),
	close: (id: string) => post<ShutterResponse>(path`/matter/covers/${id}/close`),
	stop: (id: string) => post<ShutterResponse>(path`/matter/covers/${id}/stop`),
	setPosition: (id: string, openPercent: number) => post<ShutterResponse>(path`/matter/covers/${id}/position`, { openPercent }),
	rename: (id: string, name: string) => patch<ShutterResponse>(path`/matter/covers/${id}`, { name }),
	remove: (id: string) => del<{ success: boolean }>(path`/matter/covers/${id}`),
	setSchedule: (id: string, schedule: SunSchedule) => put<ShutterResponse>(path`/matter/covers/${id}/schedule`, schedule),
	/** Skip (or no longer skip) the next scheduled opening or closing, once. */
	skip: (id: string, event: 'open' | 'close', skip: boolean) => post<ShutterResponse>(path`/matter/covers/${id}/skip`, { event, skip }),
	place: () => get<{ success: boolean; place: Place | null }>('/matter/place'),
	setPlace: (place: Place) => put<{ success: boolean; place: Place }>('/matter/place', place),
	searchPlaces: (query: string, lang: string) =>
		get<{ success: boolean; places: Place[] }>(`/matter/place/search?${new URLSearchParams({ q: query, lang })}`)
};
