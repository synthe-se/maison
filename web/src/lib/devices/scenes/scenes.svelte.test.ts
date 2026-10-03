import { afterEach, describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { remoteSources } from '#lib/test/remote.ts';
import { iconOf, sayRun, slug, templates, TEMPLATE_IDS } from './scenes.ts';

afterEach(() => (ui.toasts = []));

describe('scenes', () => {
	it('three templates filled from the house’s devices, at the shortcuts’ ids', () => {
		const [leave, night, film] = templates(remoteSources());
		expect(leave).toMatchObject({ id: TEMPLATE_IDS.leave, name: m.scenes_template_leave(), icon: 'log-out' });
		const lampsOff = [
			{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' },
			{ action: 'hue_power', lamp: 'hue-1', state: 'off' }
		];
		const closed = { action: 'cover', cover: 's1', command: 'close' };
		expect(leave.actions).toEqual([
			...lampsOff,
			{ action: 'meross_power', device: 'p1', state: 'off' },
			{ action: 'tv_power', state: 'off' },
			{ action: 'climate_off', host: '192.0.2.60' },
			closed
		]);
		expect(night).toMatchObject({ id: 'nuit', actions: [...lampsOff, closed] });
		expect(film.actions).toEqual([
			{ action: 'tv_power', state: 'on', switchToBox: true },
			{ action: 'androidtv_app', package: 'org.smarttube.beta' },
			{ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 51 }
		]);
	});

	it('dims a Hue lamp for a film when there is no Zigbee lamp; no blaster, no AC action', () => {
		const [leave, , film] = templates({ ...remoteSources(), lamps: [], hosts: [] });
		expect(film.actions.at(-1)).toEqual({ action: 'hue_brightness', lamp: 'hue-1', brightness: 20 });
		expect(leave.actions.map((a) => a.action)).not.toContain('climate_off');
	});

	it('a new scene’s id from its name, its own', () => {
		expect(slug('Je pars', [])).toBe('je-pars');
		expect(slug('Soirée télé !', [])).toBe('soiree-tele');
		expect(slug('Je pars', ['je-pars', 'je-pars-2'])).toBe('je-pars-3');
		expect(slug('!!!', [])).toBe('scene');
	});

	it('an icon it does not know is drawn as sparkles', () => {
		expect(iconOf({ icon: 'moon' })).toBe('moon');
		expect(iconOf({ icon: 'rocket' })).toBe('sparkles');
	});

	it('says a run: every action done, or the failed ones named in a warning', () => {
		sayRun({ name: 'Nuit' }, { success: true, results: ['ok: a', 'ok: b'] });
		expect(ui.toasts.at(-1)).toMatchObject({ text: m.scenes_done({ name: 'Nuit', count: 2 }), warn: false });
		sayRun({ name: 'Nuit' }, { success: false, results: ['ok: a', 'failed: Chevet timed out'] });
		expect(ui.toasts.at(-1)).toMatchObject({
			text: m.scenes_failed({ name: 'Nuit', count: 1, total: 2, details: 'Chevet timed out' }),
			warn: true
		});
	});
});
