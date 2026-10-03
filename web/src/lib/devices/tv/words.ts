// The TV's and the box's tiles in words, once (their dashboard rows and their pages).

import { m } from '#lib/paraglide/messages.js';
import type { AndroidTvStatus, TvPower, TvStatus, TvVolume } from './api.ts';

const POWER: Record<TvPower, () => string> = {
	on: m.state_on,
	standby: m.state_standby,
	deep_standby: m.tv_deep_standby
};

/** Powered up, JointSPACE silent: the backend reports « on » but reads nothing else, so the
 * state is assumed, not read (docs/ux.md § 2, case 3). */
export const assumedOn = (s: TvStatus | undefined) => s?.power === 'on' && !s.volume;

/** Line 2 of the TV's tile. */
export function tvState(s: TvStatus | undefined): string {
	if (!s) return '';
	if (!s.configured) return m.tv_not_configured();
	return assumedOn(s) ? m.tv_assumed() : POWER[s.power]();
}

/** The TV tile's one fact: the volume. */
export function tvFact(v: TvVolume | undefined): string | undefined {
	return v ? (v.muted ? m.tv_muted() : m.tv_volume_fact({ level: v.current })) : undefined;
}

/** Line 2 of the box's tile. */
export function boxState(s: AndroidTvStatus | undefined): string {
	if (!s?.configured) return m.android_tv_not_configured();
	if (!s.reachable) return m.state_unreachable();
	return s.awake ? m.android_tv_awake() : m.android_tv_asleep();
}
