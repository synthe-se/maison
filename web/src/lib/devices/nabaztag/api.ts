// The rabbit's API (garenne, backend/src/nabaztag.rs).

import { get, post, type SimpleResponse } from '#lib/api.ts';
export interface NabaztagConfig {
	host: string | null;
	tempoEnabled: boolean;
}

export interface NabaztagStatusResponse {
	success: boolean;
	config: NabaztagConfig;
	reachable: boolean;
}

export interface NabaztagTempoPushResponse {
	success: boolean;
	message: string;
	result: { todayColor: string; tomorrowColor: string | null; ledHex: string; earPosition: number | null };
}

/** The tai chi choreography, as the remote's « Taichi » key and the keymap say it (backend
 * ir.rs, ir-keymap.json.template): the rabbit's own button sends the same. */
export const TAICHI = 'chor taichi';

export const nabaztagApi = {
	status: () => get<NabaztagStatusResponse>('/nabaztag'),
	pushTempo: (forceRefresh = false) => post<NabaztagTempoPushResponse>('/nabaztag/tempo/push', { forceRefresh }),
	/** One of the rabbit's commands (« chor … », « dance »…), checked by the backend's allow-list. */
	command: (command: string) => post<SimpleResponse>('/nabaztag/ctl', { command })
};
