import { afterEach, describe, expect, it, vi } from 'vitest';
import { CONFIRM, FAILURE, TAP, haptic } from './haptics.ts';

describe('haptic', () => {
	afterEach(() => vi.unstubAllGlobals());

	it('vibrates with the given pattern, a light tick by default', () => {
		const vibrate = vi.fn(() => true);
		vi.stubGlobal('navigator', { vibrate });
		haptic();
		haptic(CONFIRM);
		haptic(FAILURE);
		expect(vibrate.mock.calls).toEqual([[TAP], [CONFIRM], [FAILURE]]);
	});

	it('distinguishes a failure from a press', () => {
		expect(Array.isArray(FAILURE)).toBe(true);
		expect(CONFIRM as number).toBeGreaterThan(TAP as number);
	});

	it('stays silent where vibration is missing (iOS, desktop)', () => {
		vi.stubGlobal('navigator', {});
		expect(() => haptic(CONFIRM)).not.toThrow();
	});

	it('swallows a webview that refuses to vibrate', () => {
		vi.stubGlobal('navigator', {
			vibrate: () => {
				throw new Error('not allowed');
			}
		});
		expect(() => haptic()).not.toThrow();
	});
});
