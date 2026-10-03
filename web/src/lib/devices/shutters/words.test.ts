import { describe as group, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { shutter } from '#lib/test/shutters.ts';
import { at, describe } from './words.ts';

group('a shutter in words', () => {
	it('closed, open, or open at a share', () => {
		expect(at(0)).toBe(m.shutters_closed());
		expect(at(100)).toBe(m.shutters_open());
		expect(at(40)).toBe(m.shutters_open_percent({ percent: 40 }));
	});

	it('out of reach, uncalibrated, moving, or where it is', () => {
		expect(describe(shutter({ online: false }))).toBe(m.state_unreachable());
		expect(describe(shutter({ openPercent: null }))).toBe(m.shutters_uncalibrated());
		expect(describe(shutter({ openPercent: 40, targetOpenPercent: 80, motion: 'opening' }))).toBe(
			`${at(40)} · ${m.shutters_going_to({ percent: 80 })}`
		);
		expect(describe(shutter({ openPercent: null, motion: 'closing' }))).toBe(m.shutters_closing());
		expect(describe(shutter({ openPercent: 40 }))).toBe(at(40));
	});
});
