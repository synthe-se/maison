// One lamp, whatever its radio: Hue over Bluetooth and Zigbee lamps are shown, worded and
// driven the same way. Only the API calls differ, gathered here in one `LampDriver` each.

import { m } from '#lib/paraglide/messages.js';
import {
	hueLampsApi,
	zigbeeLampsApi,
	type HueLamp,
	type HueLampsResponse,
	type HueLampStatusResponse,
	type LampStats,
	type ZigbeeLamp,
	type ZigbeeLampsResponse,
	type ZigbeeLampStatusResponse
} from './api.ts';
import { unreachableSince } from '#lib/format.ts';
import { source, sources, type Source } from '#lib/live.svelte.ts';

/** Polling, as the React app did: the dashboard lists every 5 s, a lamp's page every 3 s. */
export const LIST_EVERY = 5_000;
export const DETAIL_EVERY = 3_000;

export interface Lamp {
	id: string;
	name: string;
	/** The server can talk to it now (Hue: Bluetooth link up; Zigbee: answered recently). */
	reachable: boolean;
	connecting: boolean;
	lastSeen: string | null;
	isOn: boolean;
	brightness: number;
	/** 0 (warm) to 100 (cool); null when the lamp has no white tuning. */
	temperature: number | null;
	model: string | null;
	manufacturer: string;
	firmware: string | null;
}

export interface LampDriver {
	/** Prefix of every `live` key of this family: refreshing it refreshes lists and pages. */
	key: string;
	href: (id: string) => string;
	power: (id: string, on: boolean) => Promise<unknown>;
	brightness: (id: string, value: number) => Promise<unknown>;
	temperature: (id: string, value: number) => Promise<unknown>;
	/** The color temperature in words, for the slider (`aria-valuetext`). */
	temperatureText: (value: number) => string;
	/** The radio it is reached by, said on its page (the dashboard groups by type). */
	radio: () => string;
}

/** A family's driver and its live values, all under `key` (so `refresh(key)` reaches every view). */
export interface LampFamily<List, Detail> extends LampDriver {
	list: Source<List>;
	stats: Source<LampStats>;
	detail: (id: string) => Source<Detail>;
}

export const fromHue = (l: HueLamp): Lamp => ({
	...l,
	reachable: l.connected,
	isOn: l.state.isOn,
	brightness: l.state.brightness,
	temperature: l.state.temperature
});

export const fromZigbee = (l: ZigbeeLamp): Lamp => ({
	...l,
	// the coordinator does not read the firmware version
	firmware: null,
	connecting: false,
	isOn: l.state.isOn,
	brightness: l.state.brightness,
	temperature: l.state.temperature
});

const tone = (percent: number) => (percent < 34 ? m.lamps_tone_warm() : percent < 67 ? m.lamps_tone_neutral() : m.lamps_tone_cool());

/** The prefixes of each family's live keys. */
export const HUE = 'hue-lamp';
export const ZIGBEE = 'zigbee-lamp';

export const hue: LampFamily<HueLampsResponse, HueLampStatusResponse> = {
	key: HUE,
	list: source(`${HUE}:list`, hueLampsApi.list, LIST_EVERY),
	stats: source(`${HUE}:stats`, hueLampsApi.stats),
	detail: sources((id: string) => source(`${HUE}:detail:${id}`, () => hueLampsApi.status(id), DETAIL_EVERY)),
	href: (id) => `/hue-lamp/${id}`,
	power: hueLampsApi.power,
	brightness: hueLampsApi.brightness,
	temperature: hueLampsApi.temperature,
	// the Bluetooth lamps report a percentage, not a color temperature
	temperatureText: (v) => m.lamps_temperature_percent({ percent: v, tone: tone(v) }),
	radio: m.lamps_radio_bluetooth
};

// Zigbee: 0 % is the warmest white, 100 % the coolest; the same bounds as MIRED_WARM and
// MIRED_COOL in backend/src/zigbee/zcl.rs (keep them in step)
const MIRED_WARM = 500;
const MIRED_COOL = 153;
const kelvin = (percent: number) => {
	const mired = MIRED_WARM - (percent * (MIRED_WARM - MIRED_COOL)) / 100;
	return Math.round(1_000_000 / mired / 100) * 100;
};

export const zigbee: LampFamily<ZigbeeLampsResponse, ZigbeeLampStatusResponse> = {
	key: ZIGBEE,
	list: source(`${ZIGBEE}:list`, zigbeeLampsApi.list, LIST_EVERY),
	stats: source(`${ZIGBEE}:stats`, zigbeeLampsApi.stats),
	detail: sources((id: string) => source(`${ZIGBEE}:detail:${id}`, () => zigbeeLampsApi.status(id), DETAIL_EVERY)),
	href: (id) => `/zigbee-lamp/${id}`,
	power: zigbeeLampsApi.power,
	brightness: zigbeeLampsApi.brightness,
	temperature: zigbeeLampsApi.temperature,
	temperatureText: (v) => m.lamps_temperature_kelvin({ kelvin: kelvin(v), tone: tone(v) }),
	radio: m.lamps_radio_zigbee
};

/** Line 2 of a lamp's tile: « Allumée, 80 % », « Éteinte », « Injoignable depuis 12 min ». */
export function lampState(l: Lamp): string {
	if (l.connecting) return m.lamps_connecting();
	if (!l.reachable) return unreachableSince(l.lastSeen);
	return l.isOn ? m.lamps_on_percent({ percent: l.brightness }) : m.state_off();
}

/** While open: every second, for the countdown (as React did); closed: slower. */
export const PAIRING_OPEN_EVERY = 1_000;
export const PAIRING_CLOSED_EVERY = 10_000;
/** The Zigbee network's pairing window. Outside the lamps' prefix: refreshing the lamps after a
 * pairing gesture must not read the window again before the coordinator has opened it. */
export const zigbeePairing = source('zigbee-pairing', zigbeeLampsApi.pairingStatus, (d) =>
	d?.pairing.active ? PAIRING_OPEN_EVERY : PAIRING_CLOSED_EVERY
);
