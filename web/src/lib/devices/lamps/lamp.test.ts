import { afterEach, describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { hueLamp, zigbeeLamp } from '#lib/test/lamps.ts';
import { DETAIL_EVERY, LIST_EVERY, fromHue, fromZigbee, hue, lampState, zigbee } from './lamp.ts';

describe('one lamp, whatever its radio', () => {
	it('a Hue lamp is reachable when its Bluetooth link is up', () => {
		const l = fromHue(hueLamp({ connected: false, reachable: true, state: { isOn: false, brightness: 12, temperature: null } }));
		expect(l).toMatchObject({ id: 'hue-1', reachable: false, isOn: false, brightness: 12, temperature: null, connecting: false });
	});

	it('a Zigbee lamp is never « connecting »', () => {
		const l = fromZigbee(zigbeeLamp({ reachable: false }));
		expect(l).toMatchObject({ id: 'zb-1', reachable: false, connecting: false, isOn: true, brightness: 60, temperature: 40 });
	});

	it('polls the list every 5 s, a lamp’s page every 3 s', () => {
		expect([LIST_EVERY, DETAIL_EVERY]).toEqual([5000, 3000]);
	});
});

describe('lampState (line 2 of the tile)', () => {
	afterEach(() => vi.useRealTimers());

	it('on with its brightness, or off', () => {
		expect(lampState(fromHue(hueLamp()))).toBe(m.lamps_on_percent({ percent: 80 }));
		expect(lampState(fromHue(hueLamp({ state: { isOn: false } })))).toBe('Éteinte');
	});

	it('connecting, said before anything else', () => {
		expect(lampState(fromHue(hueLamp({ connected: false, connecting: true })))).toBe(m.lamps_connecting());
	});

	it('unreachable: since when, at least a minute; or never seen', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-10-02T12:00:00Z'));
		expect(lampState(fromZigbee(zigbeeLamp({ reachable: false, lastSeen: '2026-10-02T11:48:00Z' })))).toBe(
			m.state_unreachable_for({ duration: m.duration_minutes({ m: 12 }) })
		);
		expect(lampState(fromZigbee(zigbeeLamp({ reachable: false, lastSeen: '2026-10-02T11:59:59Z' })))).toBe(
			m.state_unreachable_for({ duration: m.duration_minutes({ m: 1 }) })
		);
		expect(lampState(fromZigbee(zigbeeLamp({ reachable: false, lastSeen: null })))).toBe(m.state_unreachable());
	});
});

describe('drivers', () => {
	it('link each lamp to its page', () => {
		expect(hue.href('a')).toBe('/hue-lamp/a');
		expect(zigbee.href('b')).toBe('/zigbee-lamp/b');
		expect(hue.key).not.toBe(zigbee.key);
	});

	it('send power, brightness and temperature to the family’s endpoints', async () => {
		const calls = stubFetch(() => json({ success: true }));
		await hue.power('h1', true);
		await hue.brightness('h1', 40);
		await hue.temperature('h1', 70);
		await zigbee.power('z1', false);
		await zigbee.brightness('z1', 5);
		await zigbee.temperature('z1', 0);
		expect(calls.map((c) => [c.init?.method, c.url, sentBody(c)])).toEqual([
			['POST', '/api/hue-lamps/h1/power', { enabled: true }],
			['POST', '/api/hue-lamps/h1/brightness', { brightness: 40 }],
			['POST', '/api/hue-lamps/h1/temperature', { temperature: 70 }],
			['POST', '/api/zigbee/lamps/z1/power', { enabled: false }],
			['POST', '/api/zigbee/lamps/z1/brightness', { brightness: 5 }],
			['POST', '/api/zigbee/lamps/z1/temperature', { temperature: 0 }]
		]);
	});

	it('Hue says its temperature as a percentage and a tone', () => {
		expect(hue.temperatureText(0)).toBe(m.lamps_temperature_percent({ percent: 0, tone: m.lamps_tone_warm() }));
		expect(hue.temperatureText(50)).toBe(m.lamps_temperature_percent({ percent: 50, tone: m.lamps_tone_neutral() }));
		expect(hue.temperatureText(100)).toBe(m.lamps_temperature_percent({ percent: 100, tone: m.lamps_tone_cool() }));
	});

	it('Zigbee says it in kelvins: 2000 K warm to 6500 K cool', () => {
		expect(zigbee.temperatureText(0)).toBe(m.lamps_temperature_kelvin({ kelvin: 2000, tone: m.lamps_tone_warm() }));
		expect(zigbee.temperatureText(100)).toBe(m.lamps_temperature_kelvin({ kelvin: 6500, tone: m.lamps_tone_cool() }));
		expect(zigbee.temperatureText(50)).toBe(m.lamps_temperature_kelvin({ kelvin: 3100, tone: m.lamps_tone_neutral() }));
	});
});
