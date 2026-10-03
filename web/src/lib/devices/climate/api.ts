// The IR blaster (Broadlink) and the Mitsubishi air conditioner it drives
// (backend/src/broadlink.rs, mitsubishi_ir.rs).

import { get, post } from '#lib/api.ts';
export interface BroadlinkDevice {
	host: string;
	mac: string;
	modelCode: number;
	friendlyModel: string;
	friendlyType: string;
	name: string;
	isLocked: boolean;
	kind: string;
	supportsLearning: boolean;
}

export interface BroadlinkCode {
	id: string;
	name: string;
	brand: string | null;
	model: string | null;
	command: string;
	packetBase64: string;
	packetLength: number;
	tags: string[];
	createdAt: string;
	updatedAt: string;
}

export interface BroadlinkDiscoverResponse {
	success: boolean;
	devices: BroadlinkDevice[];
	total: number;
	message: string;
}

export interface BroadlinkCodesResponse {
	success: boolean;
	codes: BroadlinkCode[];
	total: number;
	message: string;
}

export interface BroadlinkSendResponse {
	success: boolean;
	result: { host: string; codeId?: string; command?: string; packetLength: number };
	message: string;
}

export interface BroadlinkClimateSettings {
	mode: string;
	temperature: number;
	fan: string;
	vane: string;
	econo: boolean;
	stopInMinutes: number | null;
}

export interface BroadlinkClimateState {
	power: boolean;
	lastCommand: string;
	lastOnCommand: string | null;
	/** Parsed form of lastOnCommand, provided by the backend. */
	settings: BroadlinkClimateSettings | null;
	host: string;
	model: string | null;
	updatedAt: string;
}

export interface BroadlinkClimateStateResponse {
	success: boolean;
	state: BroadlinkClimateState | null;
	message: string;
}

export const broadlinkApi = {
	/** The IR blasters on the network: the server's cache, or (`force`, an admin's) a new scan. */
	discover: (force = false) => get<BroadlinkDiscoverResponse>(force ? '/broadlink/discover?forceRefresh=true' : '/broadlink/discover'),
	listCodes: () => get<BroadlinkCodesResponse>('/broadlink/codes'),
	getMitsubishiState: () => get<BroadlinkClimateStateResponse>('/broadlink/mitsubishi/state'),
	sendMitsubishiCommand: (host: string, command: string, model?: string, localIp?: string) =>
		post<BroadlinkSendResponse>('/broadlink/mitsubishi/send', { host, command, model, localIp })
};
