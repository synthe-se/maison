import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { CLIMATE_FANS, CLIMATE_MODES, CLIMATE_VANES } from './command.ts';
import { options } from '#lib/options.ts';
import { degrees, FAN_LABEL, MODE_LABEL, modeLabel, VANE_LABEL } from './labels.ts';

const modeOptions = () => options(CLIMATE_MODES, MODE_LABEL);
const fanOptions = () => options(CLIMATE_FANS, FAN_LABEL);
const vaneOptions = () => options(CLIMATE_VANES, VANE_LABEL);

describe('climate labels', () => {
	it('offers every mode, fan speed and vane position, in the grammar’s order, in words', () => {
		expect(modeOptions().map((o) => o.value)).toEqual([...CLIMATE_MODES]);
		expect(fanOptions().map((o) => o.value)).toEqual([...CLIMATE_FANS]);
		expect(vaneOptions().map((o) => o.value)).toEqual([...CLIMATE_VANES]);
		expect(modeOptions().map((o) => o.label)).toEqual([
			m.climate_modes_cool(),
			m.climate_modes_heat(),
			m.climate_modes_dry(),
			m.climate_fan(),
			m.climate_auto()
		]);
		expect(fanOptions().map((o) => o.label)).toEqual(['Auto', 'Niveau 1', 'Niveau 2', 'Niveau 3', 'Niveau 4', 'Silencieux']);
		expect(vaneOptions().find((o) => o.value === 'swing')?.label).toBe(m.climate_vanes_swing());
		for (const o of [...modeOptions(), ...fanOptions(), ...vaneOptions()]) expect(o.label).not.toBe('');
	});

	it('says a known mode in words, an unknown one as it came', () => {
		expect(modeLabel('heat')).toBe('Chaud');
		expect(modeLabel('turbo')).toBe('turbo');
	});

	it('writes degrees in the reader’s format', () => {
		expect(degrees(21)).toBe(m.climate_degrees({ degrees: '21' }));
		expect(degrees(21)).toMatch(/^21\s°C$/);
	});
});
