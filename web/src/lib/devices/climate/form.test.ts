import { describe, expect, it } from 'vitest';
import { commandOf, formFrom, INITIAL } from './form.ts';

describe('the AC form', () => {
	it('starts from the initial settings without a stored order', () => {
		expect(formFrom(null)).toEqual(INITIAL);
		expect(commandOf(INITIAL)).toBe('state-cool-20-fan-auto-vane-auto-wide-center');
	});

	it('fills from the last order the server kept, its timer on when it had one', () => {
		const f = formFrom({ mode: 'heat', temperature: 23, fan: '2', vane: 'low', econo: false, stopInMinutes: 90 });
		expect(f).toMatchObject({ mode: 'heat', temperature: 23, fan: '2', vane: 'low', timer: true, stopAfter: 90 });
		expect(commandOf(f)).toBe('state-heat-23-fan-2-vane-low-wide-center-stopin-90');
		expect(commandOf({ ...f, timer: false })).toBe('state-heat-23-fan-2-vane-low-wide-center');
	});

	it('unknown values fall back one by one', () => {
		expect(formFrom({ mode: 'turbo', temperature: 99, fan: 'x', vane: 'swing', econo: true, stopInMinutes: null })).toMatchObject({
			mode: 'cool',
			temperature: 31,
			fan: 'auto',
			vane: 'swing',
			econo: true,
			timer: false
		});
	});
});
