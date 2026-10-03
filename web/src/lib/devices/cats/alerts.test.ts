import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { clock, when } from '#lib/i18n.svelte.ts';
import { lastMeal, statusLine, tileLine } from './alerts.ts';

describe('statusLine', () => {
	it('nothing to say without a status, or when all is well', () => {
		expect(statusLine('fountain', undefined)).toBeNull();
		expect(statusLine('fountain', { waterLevel: 'ok' })).toBeNull();
		expect(statusLine('feeder', { system: { poweredBy: 'AC Power' } })).toBeNull();
		expect(statusLine('unknown', {})).toBeNull();
	});

	it('what needs a hand, in the warning style', () => {
		expect(statusLine('fountain', { waterLevel: 'low' })).toEqual({ text: m.cats_water_low(), warn: true });
		expect(statusLine('litter-box', { sensors: { litterLevel: 'half' }, system: { state: 'satnd_by' } })).toEqual({
			text: m.cats_litter_half(),
			warn: true
		});
		expect(statusLine('litter-box', { system: { maintenanceRequired: true } })).toEqual({
			text: m.litter_box_maintenance_required(),
			warn: true
		});
		expect(statusLine('litter-box', { sensors: { faultAlarm: 3 } })).toEqual({ text: m.litter_box_fault_alarm({ code: 3 }), warn: true });
	});

	it('what the litter box is doing, plainly', () => {
		expect(statusLine('litter-box', { sensors: { litterLevel: 'full' }, system: { state: 'satnd_by' } })).toEqual({
			text: m.cats_clean(),
			warn: false
		});
		expect(statusLine('litter-box', { system: { state: 'cleaning' } })).toEqual({ text: m.litter_box_status_cleaning(), warn: false });
		expect(statusLine('litter-box', { system: { state: 'cat_inside' } })).toEqual({ text: m.litter_box_status_cat_inside(), warn: false });
	});
});

describe('tileLine', () => {
	it('offline first, in the warning style', () => {
		expect(tileLine({ type: 'fountain', connected: false }, { waterLevel: 'low' })).toEqual({ text: m.device_offline(), warn: true });
	});

	it('then what the status says, then the meal served from here, then « En ligne »', () => {
		expect(tileLine({ type: 'fountain', connected: true }, { waterLevel: 'low' }).text).toBe(m.cats_water_low());
		const at = new Date(2026, 9, 3, 8, 5).getTime();
		expect(tileLine({ type: 'feeder', connected: true }, {}, at)).toEqual({ text: m.feeder_served_at({ time: clock(at) }), warn: false });
		expect(tileLine({ type: 'feeder', connected: true }, undefined)).toEqual({ text: m.device_online(), warn: false });
	});
});

describe('lastMeal', () => {
	it('says the portions and when, from the feeder’s report', () => {
		const s = {
			history: { raw: 'R:0 C:2 T:1773270006', parsed: { remaining: '0', count: '2', timestamp: '1773270006', timestampReadable: '' } }
		};
		expect(lastMeal(s)).toBe(m.feeder_last_meal_value({ portions: m.feeder_portion({ count: 2 }), when: when(1773270006000) }));
	});

	it('nothing without a report, or with one it cannot read', () => {
		expect(lastMeal(undefined)).toBeNull();
		expect(lastMeal({ history: null })).toBeNull();
		expect(lastMeal({ history: { raw: 'C:1', parsed: { remaining: '', count: '1', timestamp: null, timestampReadable: '' } } })).toBeNull();
	});
});
