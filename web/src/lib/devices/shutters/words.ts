// A shutter's state in words, once (the tile's line 2, the slider's value): « Fermé »,
// « Ouvert à 40 % », « Ouvert », and while its motor runs where it is and where it goes.

import { m } from '#lib/paraglide/messages.js';
import type { Shutter } from './api.ts';
import { moving } from './data.ts';

/** « Fermé », « Ouvert à 40 % », « Ouvert ». */
export function at(percent: number): string {
	if (percent >= 100) return m.shutters_open();
	if (percent <= 0) return m.shutters_closed();
	return m.shutters_open_percent({ percent });
}

/** Line 2 of a shutter's tile. */
export function describe(c: Shutter): string {
	if (!c.online) return m.state_unreachable();
	const going = c.targetOpenPercent !== null && c.openPercent !== null && c.targetOpenPercent !== c.openPercent;
	if (moving(c) && going) return `${at(c.openPercent!)} · ${m.shutters_going_to({ percent: c.targetOpenPercent! })}`;
	if (c.motion === 'opening') return m.shutters_opening();
	if (c.motion === 'closing') return m.shutters_closing();
	if (c.openPercent === null) return m.shutters_uncalibrated();
	return at(c.openPercent);
}
