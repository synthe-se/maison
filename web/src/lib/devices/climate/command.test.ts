import { describe, expect, it } from 'vitest';
import {
	CLIMATE_OFF,
	buildClimateCommand,
	isClimateMode,
	parseClimateCommand,
	settingsFromBackend,
	type ClimateSettings
} from './command.ts';

const base: ClimateSettings = { mode: 'cool', temperature: 20, fan: 'auto', vane: 'auto' };

describe('buildClimateCommand', () => {
	it('writes mode, setpoint, fan and vane, in that order', () => {
		expect(buildClimateCommand(base)).toBe('state-cool-20-fan-auto-vane-auto');
	});

	it('adds the optional parts only when set, in the grammar’s order', () => {
		expect(
			buildClimateCommand({
				...base,
				mode: 'heat',
				temperature: 23,
				fan: '3',
				vane: 'swing',
				wide: 'center',
				econo: true,
				stopInMinutes: 90
			})
		).toBe('state-heat-23-fan-3-vane-swing-wide-center-econo-on-stopin-90');
		expect(buildClimateCommand({ ...base, econo: false, stopInMinutes: null, wide: '' })).toBe('state-cool-20-fan-auto-vane-auto');
		expect(buildClimateCommand({ ...base, stopInMinutes: 0 })).toBe('state-cool-20-fan-auto-vane-auto');
	});
});

describe('parseClimateCommand', () => {
	it('reads back what build writes (round trip)', () => {
		const all: ClimateSettings[] = [
			{ ...base, wide: undefined, econo: false, stopInMinutes: null },
			{ mode: 'heat', temperature: 31, fan: 'silent', vane: 'lowest', wide: 'center', econo: true, stopInMinutes: 180 },
			{ mode: 'dry', temperature: 16, fan: '1', vane: 'highest', wide: undefined, econo: false, stopInMinutes: 10 },
			{ mode: 'fan', temperature: 24, fan: '4', vane: 'middle', wide: 'left', econo: false, stopInMinutes: null }
		];
		for (const s of all) expect(parseClimateCommand(buildClimateCommand(s))).toEqual(s);
	});

	it('reads econo off explicitly', () => {
		expect(parseClimateCommand('state-auto-21-fan-2-vane-high-econo-off')).toMatchObject({ mode: 'auto', econo: false });
	});

	it('has no settings for « off »', () => {
		expect(parseClimateCommand(CLIMATE_OFF)).toBeNull();
	});

	it('refuses what it does not model: unknown tokens, absolute stops, junk', () => {
		expect(parseClimateCommand('state-turbo-20-fan-auto-vane-auto')).toBeNull();
		expect(parseClimateCommand('state-cool-20-fan-9-vane-auto')).toBeNull();
		expect(parseClimateCommand('state-cool-20-fan-auto-vane-sideways')).toBeNull();
		expect(parseClimateCommand('state-cool-20-fan-auto-vane-auto-stop-22-30')).toBeNull();
		expect(parseClimateCommand('state-cool-20-fan-auto-vane-auto-timer-off')).toBeNull();
		expect(parseClimateCommand('state-cool-20-fan-auto-vane-auto-stopin-10-wide-left')).toBeNull();
		expect(parseClimateCommand('')).toBeNull();
	});
});

describe('guards and bounds', () => {
	it('knows its tokens', () => {
		expect(isClimateMode('heat')).toBe(true);
		expect(isClimateMode('turbo')).toBe(false);
		expect(parseClimateCommand('state-cool-20-fan-silent-vane-swing')).toMatchObject({ fan: 'silent', vane: 'swing' });
		expect(parseClimateCommand('state-cool-20-fan-5-vane-auto')).toBeNull();
		expect(parseClimateCommand('state-cool-20-fan-auto-vane-left')).toBeNull();
	});

	it('keeps a stored setpoint within 16–31 °C, rounded', () => {
		const said = (temperature: number) =>
			settingsFromBackend({ mode: 'cool', temperature, fan: 'auto', vane: 'auto', econo: false, stopInMinutes: null }, base).temperature;
		expect(said(10)).toBe(16);
		expect(said(40)).toBe(31);
		expect(said(21.6)).toBe(22);
	});
});

describe('settingsFromBackend', () => {
	const fallback: ClimateSettings = { ...base, wide: 'center', econo: false, stopInMinutes: null };

	it('takes every known value', () => {
		expect(settingsFromBackend({ mode: 'heat', temperature: 24, fan: '2', vane: 'low', econo: true, stopInMinutes: 60 }, fallback)).toEqual(
			{
				mode: 'heat',
				temperature: 24,
				fan: '2',
				vane: 'low',
				wide: 'center',
				econo: true,
				stopInMinutes: 60
			}
		);
	});

	it('falls back one value at a time, and clamps the setpoint', () => {
		expect(
			settingsFromBackend({ mode: 'turbo', temperature: 99, fan: 'max', vane: 'left', econo: false, stopInMinutes: null }, fallback)
		).toEqual({
			...fallback,
			temperature: 31
		});
	});
});
