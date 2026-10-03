import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import type { IrAction } from '#lib/devices/remote/api.ts';
import { degrees, FAN_LABEL, MODE_LABEL, VANE_LABEL } from '#lib/devices/climate/labels.ts';
import { remoteSources } from '#lib/test/remote.ts';
import {
	ACTION_TYPES,
	defaultAction,
	isComplete,
	isEditable,
	outcome,
	SCENE_TYPES,
	summarize,
	type ActionType,
	type Sources
} from './actions.ts';

const sources = remoteSources();
const none: Sources = { lamps: [], hueLamps: [], covers: [], plugs: [], hosts: [], codes: [], apps: [], scenes: [] };

describe('summarize', () => {
	it('says each action type in one line, with the devices’ real names', () => {
		expect(summarize(<IrAction>{ action: 'nabaztag', command: 'ping' }, sources)).toBe(m.remote_summary_nabaztag({ command: 'ping' }));
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: 'zb-1', state: 'on' }, sources)).toBe('Allumer Suspension');
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' }, sources)).toBe('Éteindre Suspension');
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: 'zb-1', state: 'toggle' }, sources)).toBe('Basculer Suspension');
		expect(summarize(<IrAction>{ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 127 }, sources)).toBe(
			m.remote_summary_zigbee_brightness({ lamp: 'Suspension', brightness: m.remote_brightness_value({ value: 127 }) })
		);
		expect(summarize(<IrAction>{ action: 'broadlink_code', host: '192.0.2.60', codeId: 'fan-on' }, sources)).toBe(
			m.remote_summary_broadlink({ host: 'RM4 salon', code: 'Ventilateur' })
		);
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'p1', state: 'on' }, sources)).toBe('Allumer la prise Radiateur');
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'p1', state: 'off' }, sources)).toBe('Éteindre la prise Radiateur');
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'p1', state: 'toggle' }, sources)).toBe('Basculer la prise Radiateur');
	});

	it('says a climate command’s settings in words', () => {
		const said = summarize(
			<IrAction>{ action: 'climate_toggle', host: '192.0.2.60', onCommand: 'state-heat-21-fan-auto-vane-low' },
			sources
		);
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
		const said = summarize(
			<IrAction>{ action: 'climate_toggle', host: '192.0.2.60', onCommand: 'state-cool-20-fan-1-vane-auto-stop-22-30' },
			sources
		);
		expect(said).toBe(m.remote_summary_climate({ host: 'RM4 salon', settings: 'state-cool-20-fan-1-vane-auto-stop-22-30' }));
	});

	it('falls back to the id of a device that is gone', () => {
		expect(summarize(<IrAction>{ action: 'zigbee_power', lamp: '0x99', state: 'on' }, none)).toBe('Allumer 0x99');
		expect(summarize(<IrAction>{ action: 'zigbee_brightness', lamp: '0x99', brightness: 1 }, none)).toContain('0x99');
		expect(summarize(<IrAction>{ action: 'broadlink_code', host: 'h', codeId: 'k' }, none)).toBe(
			m.remote_summary_broadlink({ host: 'h', code: 'k' })
		);
		expect(summarize(<IrAction>{ action: 'meross_power', device: 'gone', state: 'toggle' }, none)).toBe('Basculer la prise gone');
		expect(summarize(<IrAction>{ action: 'climate_toggle', host: 'h', onCommand: 'state-off' }, none)).toBe(
			m.remote_summary_climate({ host: 'h', settings: 'state-off' })
		);
	});

	it('names an action the configurator does not edit, and says it is kept', () => {
		expect(isEditable({ action: 'tv_key' })).toBe(false);
		expect(summarize({ action: 'tv_key' }, sources)).toBe(m.remote_summary_other({ action: 'tv_key' }));
	});
});

describe('defaultAction', () => {
	it('starts every type on the first device offered', () => {
		expect(defaultAction('nabaztag', sources)).toEqual({ action: 'nabaztag', command: '' });
		expect(defaultAction('zigbee_power', sources)).toEqual({ action: 'zigbee_power', lamp: 'zb-1', state: 'toggle' });
		expect(defaultAction('zigbee_brightness', sources)).toEqual({ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 127 });
		expect(defaultAction('broadlink_code', sources)).toEqual({ action: 'broadlink_code', host: '192.0.2.60', codeId: 'fan-on' });
		expect(defaultAction('meross_power', sources)).toEqual({ action: 'meross_power', device: 'p1', state: 'toggle' });
		expect(defaultAction('climate_toggle', sources)).toEqual({
			action: 'climate_toggle',
			host: '192.0.2.60',
			onCommand: 'state-cool-16-fan-4-vane-swing'
		});
	});

	it('leaves the device empty, hence incomplete, when there is none', () => {
		// the TV needs no device to name: always complete
		for (const type of (Object.keys(ACTION_TYPES) as ActionType[]).filter((t) => t !== 'tv_power')) {
			expect(isComplete(defaultAction(type, none)), type).toBe(false);
		}
	});

	it('starts the TV on (on the box), the box on its first app, a scene on the first scene', () => {
		expect(defaultAction('tv_power', sources)).toEqual({ action: 'tv_power', state: 'on', switchToBox: true });
		expect(defaultAction('androidtv_app', sources)).toEqual({ action: 'androidtv_app', package: 'org.smarttube.beta' });
		expect(defaultAction('scene', sources)).toEqual({ action: 'scene', scene: 'nuit' });
	});

	it('a scene may hold every type but a scene', () => {
		expect(SCENE_TYPES).not.toContain('scene');
		expect(SCENE_TYPES).toContain('tv_power');
	});
});

describe('Hue lamps, shutters and the AC, summed up', () => {
	it('with their real names, worded like the other lamps and the shutter tile', () => {
		expect(summarize(<IrAction>{ action: 'hue_power', lamp: 'hue-1', state: 'off' }, sources)).toBe('Éteindre Lampe du salon');
		expect(summarize(<IrAction>{ action: 'hue_power', lamp: 'hue-1', state: 'toggle' }, sources)).toBe('Basculer Lampe du salon');
		expect(summarize(<IrAction>{ action: 'hue_brightness', lamp: 'hue-1', brightness: 20 }, sources)).toBe(
			'Lampe du salon\u00a0: luminosité 20\u00a0%'
		);
		expect(summarize(<IrAction>{ action: 'cover', cover: 's1', command: 'close' }, sources)).toBe('Fermer Salon');
		expect(summarize(<IrAction>{ action: 'cover', cover: 's1', command: 'open' }, sources)).toBe('Ouvrir Salon');
		expect(summarize(<IrAction>{ action: 'cover', cover: 's1', command: 'stop' }, sources)).toBe('Arrêter Salon');
		expect(summarize(<IrAction>{ action: 'cover', cover: 's1', command: 'position', position: 30 }, sources)).toBe(
			'Salon\u00a0: Ouvert à 30\u202f%'
		);
		expect(summarize(<IrAction>{ action: 'climate_off', host: '192.0.2.60' }, sources)).toBe('Éteindre la clim via RM4 salon');
		expect(summarize(<IrAction>{ action: 'climate_on', host: '192.0.2.60' }, sources)).toBe(
			'Rallumer la clim via RM4 salon (derniers réglages)'
		);
	});

	it('falls back to the ids of devices that are gone', () => {
		expect(summarize(<IrAction>{ action: 'hue_power', lamp: 'gone', state: 'on' }, none)).toBe('Allumer gone');
		expect(summarize(<IrAction>{ action: 'cover', cover: 'gone', command: 'close' }, none)).toBe('Fermer gone');
		expect(summarize(<IrAction>{ action: 'climate_off', host: 'h' }, none)).toBe('Éteindre la clim via h');
	});

	it('start on the first device offered: a Hue lamp toggled or halfway, a shutter closed, the first blaster', () => {
		expect(defaultAction('hue_power', sources)).toEqual({ action: 'hue_power', lamp: 'hue-1', state: 'toggle' });
		expect(defaultAction('hue_brightness', sources)).toEqual({ action: 'hue_brightness', lamp: 'hue-1', brightness: 50 });
		expect(defaultAction('cover', sources)).toEqual({ action: 'cover', cover: 's1', command: 'close' });
		expect(defaultAction('climate_off', sources)).toEqual({ action: 'climate_off', host: '192.0.2.60' });
		expect(defaultAction('climate_on', sources)).toEqual({ action: 'climate_on', host: '192.0.2.60' });
	});

	it('a shutter sent to a position needs the position', () => {
		expect(isComplete(<IrAction>{ action: 'cover', cover: 's1', command: 'position', position: 0 })).toBe(true);
		expect(isComplete(<IrAction>{ action: 'cover', cover: 's1', command: 'position' })).toBe(false);
		expect(isComplete(<IrAction>{ action: 'cover', cover: '', command: 'close' })).toBe(false);
	});
});

describe('the TV, the box and scenes, summed up', () => {
	it('with their real names', () => {
		expect(summarize(<IrAction>{ action: 'tv_power', state: 'off' }, sources)).toBe(m.remote_summary_tv_off());
		expect(summarize(<IrAction>{ action: 'androidtv_app', package: 'org.smarttube.beta' }, sources)).toBe(
			m.remote_summary_androidtv_app({ app: 'SmartTube' })
		);
		expect(summarize(<IrAction>{ action: 'scene', scene: 'nuit' }, sources)).toBe(m.remote_summary_scene({ scene: 'Nuit' }));
		expect(summarize(<IrAction>{ action: 'scene', scene: 'gone' }, sources)).toBe(m.remote_summary_scene({ scene: 'gone' }));
	});

	it('reads a run’s results', () => {
		expect(outcome('ok: lamp off')).toEqual({ ok: true, detail: 'lamp off' });
		expect(outcome('failed: timeout')).toEqual({ ok: false, detail: 'timeout' });
	});
});

describe('isComplete', () => {
	it('asks for what each type needs', () => {
		const complete: IrAction[] = [
			{ action: 'nabaztag', command: 'ping' },
			{ action: 'zigbee_power', lamp: 'zb-1', state: 'on' },
			{ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 0 },
			{ action: 'broadlink_code', host: 'h', codeId: 'c' },
			{ action: 'meross_power', device: 'p', state: 'off' },
			{ action: 'climate_toggle', host: 'h', onCommand: 'state-off' }
		];
		for (const a of complete) expect(isComplete(a), a.action).toBe(true);
		expect(isComplete(<IrAction>{ action: 'nabaztag', command: '   ' })).toBe(false);
		expect(isComplete(<IrAction>{ action: 'broadlink_code', host: 'h', codeId: '' })).toBe(false);
		expect(isComplete(<IrAction>{ action: 'climate_toggle', host: 'h', onCommand: '' })).toBe(false);
	});

	it('never blocks a save on an action it does not edit', () => {
		expect(isComplete({ action: 'androidtv_key' })).toBe(true);
	});
});
