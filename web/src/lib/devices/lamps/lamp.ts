// One lamp, whatever its radio: Hue over Bluetooth and Zigbee lamps are shown, worded and
// driven the same way. Only the API calls differ, gathered here in one `LampDriver` each.

import { m } from '#lib/paraglide/messages.js';
import { hueLampsApi, zigbeeLampsApi, type HueLamp, type ZigbeeLamp } from '#lib/api.ts';
import { unreachableSince } from '#lib/format.ts';

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
	/** The family's `live` keys, all under `key` (so `refresh(key)` reaches every view). */
	keys: { list: string; stats: string; detail: (id: string) => string };
	href: (id: string) => string;
	power: (id: string, on: boolean) => Promise<unknown>;
	brightness: (id: string, value: number) => Promise<unknown>;
	temperature: (id: string, value: number) => Promise<unknown>;
	/** The colour temperature in words, for the slider (`aria-valuetext`). */
	temperatureText: (value: number) => string;
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

const tone = (percent: number) =>
	percent < 34 ? m.lamps_tone_warm() : percent < 67 ? m.lamps_tone_neutral() : m.lamps_tone_cool();

const keysOf = (key: string): LampDriver['keys'] => ({ list: `${key}:list`, stats: `${key}:stats`, detail: (id) => `${key}:${id}` });

export const hue: LampDriver = {
	key: 'hue-lamp',
	keys: keysOf('hue-lamp'),
	href: (id) => `/hue-lamp/${id}`,
	power: hueLampsApi.power,
	brightness: hueLampsApi.brightness,
	temperature: hueLampsApi.temperature,
	// the Bluetooth lamps report a percentage, not a colour temperature
	temperatureText: (v) => m.lamps_temperature_percent({ percent: v, tone: tone(v) })
};

// Zigbee: 0 % is the warmest white, 100 % the coolest; the same bounds as MIRED_WARM and
// MIRED_COOL in backend/src/zigbee_native.rs (keep them in step)
const MIRED_WARM = 500;
const MIRED_COOL = 153;
const kelvin = (percent: number) => {
	const mired = MIRED_WARM - (percent * (MIRED_WARM - MIRED_COOL)) / 100;
	return Math.round(1_000_000 / mired / 100) * 100;
};

export const zigbee: LampDriver = {
	key: 'zigbee-lamp',
	keys: keysOf('zigbee-lamp'),
	href: (id) => `/zigbee-lamp/${id}`,
	power: zigbeeLampsApi.power,
	brightness: zigbeeLampsApi.brightness,
	temperature: zigbeeLampsApi.temperature,
	temperatureText: (v) => m.lamps_temperature_kelvin({ kelvin: kelvin(v), tone: tone(v) })
};

/** Line 2 of a lamp's tile: « Allumée, 80 % », « Éteinte », « Injoignable depuis 12 min ». */
export function lampState(l: Lamp): string {
	if (l.connecting) return m.lamps_connecting();
	if (!l.reachable) return unreachableSince(l.lastSeen);
	return l.isOn ? m.lamps_on_percent({ percent: l.brightness }) : m.state_off();
}
