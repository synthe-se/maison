// The air conditioner's form, as the tile keeps it: the settings, plus whether an off timer is
// asked and after how long; what it starts with, and what it becomes from the last order the
// server kept (AGENTS.md « Climate »: the form restores it once).

import type { BroadlinkClimateSettings } from './api.ts';
import { buildClimateCommand, settingsFromBackend, type ClimateSettings } from './command.ts';

export type ClimateForm = ClimateSettings & { econo: boolean; timer: boolean; stopAfter: number };

export const INITIAL: ClimateForm = {
	mode: 'cool',
	temperature: 20,
	fan: 'auto',
	vane: 'auto',
	econo: false,
	timer: false,
	stopAfter: 180
};

/** The form for the last order the server kept; the initial one without it. */
export function formFrom(settings: BroadlinkClimateSettings | null | undefined): ClimateForm {
	if (!settings) return { ...INITIAL };
	const restored = settingsFromBackend(settings, INITIAL);
	return {
		...restored,
		econo: restored.econo ?? false,
		timer: !!restored.stopInMinutes,
		stopAfter: restored.stopInMinutes ?? INITIAL.stopAfter
	};
}

/** The order the form makes (the living-room unit's horizontal vane stays centred). */
export const commandOf = (f: ClimateForm) => buildClimateCommand({ ...f, wide: 'center', stopInMinutes: f.timer ? f.stopAfter : null });
