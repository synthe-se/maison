import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import type { AndroidTvStatus, TvStatus } from './api.ts';
import { assumedOn, boxState, tvFact, tvState } from './words.ts';

const tv = (over: Partial<TvStatus> = {}): TvStatus => ({
	configured: true,
	power: 'on',
	volume: { current: 12, min: 0, max: 60, muted: false },
	...over
});
const box = (over: Partial<AndroidTvStatus> = {}): AndroidTvStatus => ({
	configured: true,
	reachable: true,
	awake: true,
	paired: true,
	...over
});

describe('the TV in words', () => {
	it('not configured, on, standby, deep standby', () => {
		expect(tvState(tv({ configured: false }))).toBe(m.tv_not_configured());
		expect(tvState(tv())).toBe(m.state_on());
		expect(tvState(tv({ power: 'standby' }))).toBe(m.state_standby());
		expect(tvState(tv({ power: 'deep_standby' }))).toBe(m.tv_deep_standby());
	});

	it('on but silent: assumed, not read', () => {
		expect(assumedOn(tv({ volume: undefined }))).toBe(true);
		expect(tvState(tv({ volume: undefined }))).toBe(m.tv_assumed());
		expect(assumedOn(tv({ power: 'standby', volume: undefined }))).toBe(false);
	});

	it('its fact is the volume, or muted', () => {
		expect(tvFact({ current: 12, min: 0, max: 60, muted: false })).toBe(m.tv_volume_fact({ level: 12 }));
		expect(tvFact({ current: 12, min: 0, max: 60, muted: true })).toBe(m.tv_muted());
		expect(tvFact(undefined)).toBeUndefined();
	});
});

describe('the box in words', () => {
	it('not configured, out of reach, awake or asleep', () => {
		expect(boxState(box({ configured: false }))).toBe(m.android_tv_not_configured());
		expect(boxState(box({ reachable: false }))).toBe(m.state_unreachable());
		expect(boxState(box())).toBe(m.android_tv_awake());
		expect(boxState(box({ awake: false }))).toBe(m.android_tv_asleep());
	});
});
