// What a cat device has to say on the dashboard, once (the tile's line 2, the « Maintenant »
// strip): what needs a hand first (« Eau basse », « Litière à moitié pleine »), in the warning
// style; then what it is doing (« Propre », « Nettoyage »); « En ligne » only when there is
// nothing to say. Read from the status endpoints the device pages already poll.

import { m } from '#lib/paraglide/messages.js';
import { clock, when } from '#lib/i18n.svelte.ts';
import type { Device, FeederStatus, FountainStatus, LitterBoxStatus } from './api.ts';
import type { Kind } from './data.ts';

export interface Line {
	text: string;
	/** Needs a hand: said in the warning style, and in the « Maintenant » strip. */
	warn: boolean;
}

const warn = (text: string): Line => ({ text, warn: true });
const fact = (text: string): Line => ({ text, warn: false });

/** The device's own line from its status; null when it has nothing to say. */
export function statusLine(kind: Kind, status: unknown): Line | null {
	if (!status) return null;
	if (kind === 'feeder') {
		const s = status as FeederStatus;
		return s.system?.faultStatus ? warn(m.cats_fault()) : null;
	}
	if (kind === 'fountain') {
		const s = status as FountainStatus;
		return s.waterLevel === 'low' ? warn(m.cats_water_low()) : null;
	}
	if (kind === 'litter-box') {
		const s = status as LitterBoxStatus;
		if (s.sensors?.faultAlarm) return warn(m.litter_box_fault_alarm({ code: s.sensors.faultAlarm }));
		if (s.system?.maintenanceRequired) return warn(m.litter_box_maintenance_required());
		if (s.sensors?.litterLevel === 'half') return warn(m.cats_litter_half());
		if (s.system?.state === 'cleaning') return fact(m.litter_box_status_cleaning());
		if (s.system?.state === 'cat_inside') return fact(m.litter_box_status_cat_inside());
		// waiting between two visits: the litter has been cleaned
		if (s.system?.state === 'satnd_by') return fact(m.cats_clean());
	}
	return null;
}

/** Line 2 of a cat device's tile: « Hors ligne », what its status says, when this screen last
 * fed it (`servedAt`, ms), else « En ligne ». */
export function tileLine(device: Pick<Device, 'type' | 'connected'>, status: unknown, servedAt?: number): Line {
	if (!device.connected) return warn(m.device_offline());
	const said = statusLine(device.type, status);
	if (said) return said;
	return fact(servedAt ? m.feeder_served_at({ time: clock(servedAt) }) : m.device_online());
}

/** The feeder's last meal in words (« 2 portions, 08:00 »), from its report; null without one. */
export function lastMeal(s: FeederStatus | undefined): string | null {
	const p = s?.history?.parsed;
	const count = Number(p?.count);
	const at = Number(p?.timestamp) * 1000;
	if (!p || !Number.isFinite(count) || !(at > 0)) return null;
	return m.feeder_last_meal_value({ portions: m.feeder_portion({ count }), when: when(at) });
}
