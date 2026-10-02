import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import type { IrAction } from '#lib/api.ts';
import { degrees, FAN_LABEL, MODE_LABEL, VANE_LABEL } from '#lib/devices/climate/labels.ts';
import { remoteSources } from '#lib/test/remote.ts';
import { ACTION_TYPES, defaultAction, isComplete, isEditable, summarize, type ActionType, type Sources } from './actions.ts';

const sources = remoteSources();
const none: Sources = { lamps: [], plugs: [], hosts: [], codes: [] };

describe('summarize', () => {
	it('says each action type in one line, with the devices’ real names', () => {
		expect(summarize(<IrAction>{ action: 'nabaztag', command: 'ping' }, sources)).toBe(m.remote_summary_nabaztag({ command: 'ping' }));
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: 'zb-1', state: 'on' }, sources)).toBe('Allumer Suspension');
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' }, sources)).toBe('Éteindre Suspension');
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: 'zb-1', state: 'toggle' }, sources)).toBe('Basculer Suspension');
		expect(summarize(<IrAction>{ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 127 }, sources)).toBe(
			m.remote_summary_zigbee_brightness({ lamp: 'Suspension', brightness: m.remote_brightness_value({ value: 127 }) })
		);
		expect(summarize(<IrAction>{ action: 'broadlink_code', host: '192.168.1.60', code_id: 'fan-on' }, sources)).toBe(
			m.remote_summary_broadlink({ host: 'RM4 salon', code: 'Ventilateur' })
		);
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'p1', state: 'on' }, sources)).toBe('Allumer la prise Radiateur');
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'p1', state: 'off' }, sources)).toBe('Éteindre la prise Radiateur');
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'p1', state: 'toggle' }, sources)).toBe('Basculer la prise Radiateur');
	});

	it('says a climate command’s settings in words', () => {
		const said = summarize(<IrAction>{ action: 'climate_toggle', host: '192.168.1.60', on_command: 'state-heat-21-fan-auto-vane-low' }, sources);
		expect(said).toBe(
			m.remote_summary_climate({
				host: 'RM4 salon',
				settings: m.remote_summary_climate_settings({
					mode: MODE_LABEL.heat(),
					temperature: degrees(21),
					fan: FAN_LABEL.auto(),
					vane: VANE_LABEL.low()
				})
			})
		);
	});

	it('keeps a climate command it cannot read as it is', () => {
		const said = summarize(<IrAction>{ action: 'climate_toggle', host: '192.168.1.60', on_command: 'state-cool-20-fan-1-vane-auto-stop-22-30' }, sources);
		expect(said).toBe(m.remote_summary_climate({ host: 'RM4 salon', settings: 'state-cool-20-fan-1-vane-auto-stop-22-30' }));
	});

	it('falls back to the id of a device that is gone', () => {
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: '0x99', state: 'on' }, none)).toBe('Allumer 0x99');
		expect(summarize(<IrAction>{ action: 'zigbee_brightness', lamp: '0x99', brightness: 1 }, none)).toContain('0x99');
		expect(summarize(<IrAction>{ action: 'broadlink_code', host: 'h', code_id: 'k' }, none)).toBe(m.remote_summary_broadlink({ host: 'h', code: 'k' }));
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'gone', state: 'toggle' }, none)).toBe('Basculer la prise gone');
		expect(summarize(<IrAction>{ action: 'climate_toggle', host: 'h', on_command: 'state-off' }, none)).toBe(
			m.remote_summary_climate({ host: 'h', settings: 'state-off' })
		);
	});

	it('names an action the configurator does not edit, and says it is kept', () => {
		expect(isEditable({ action: 'tv_power' })).toBe(false);
		expect(summarize({ action: 'tv_power' }, sources)).toBe(m.remote_summary_other({ action: 'tv_power' }));
	});
});

describe('defaultAction', () => {
	it('starts every type on the first device offered', () => {
		expect(defaultAction('nabaztag', sources)).toEqual({ action: 'nabaztag', command: '' });
		expect(defaultAction('zigbee_power', sources)).toEqual({ action: 'zigbee_power', lamp: 'zb-1', state: 'toggle' });
		expect(defaultAction('zigbee_brightness', sources)).toEqual({ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 127 });
		expect(defaultAction('broadlink_code', sources)).toEqual({ action: 'broadlink_code', host: '192.168.1.60', code_id: 'fan-on' });
		expect(defaultAction('meross_power', sources)).toEqual({ action: 'meross_power', device: 'p1', state: 'toggle' });
		expect(defaultAction('climate_toggle', sources)).toEqual({
			action: 'climate_toggle',
			host: '192.168.1.60',
			on_command: 'state-cool-16-fan-4-vane-swing'
		});
	});

	it('leaves the device empty, hence incomplete, when there is none', () => {
		for (const type of Object.keys(ACTION_TYPES) as ActionType[]) {
			expect(isComplete(defaultAction(type, none)), type).toBe(false);
		}
	});
});

describe('isComplete', () => {
	it('asks for what each type needs', () => {
		const complete: IrAction[] = [
			{ action: 'nabaztag', command: 'ping' },
			{ action: 'zigbee_power', lamp: 'zb-1', state: 'on' },
			{ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 0 },
			{ action: 'broadlink_code', host: 'h', code_id: 'c' },
			{ action: 'meross_power', device: 'p', state: 'off' },
			{ action: 'climate_toggle', host: 'h', on_command: 'state-off' }
		];
		for (const a of complete) expect(isComplete(a), a.action).toBe(true);
		expect(isComplete(<IrAction>{ action: 'nabaztag', command: '   ' })).toBe(false);
		expect(isComplete(<IrAction>{ action: 'broadlink_code', host: 'h', code_id: '' })).toBe(false);
		expect(isComplete(<IrAction>{ action: 'climate_toggle', host: 'h', on_command: '' })).toBe(false);
	});

	it('never blocks a save on an action it does not edit', () => {
		expect(isComplete({ action: 'androidtv_key' })).toBe(true);
	});
});
