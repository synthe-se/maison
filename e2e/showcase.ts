// The README's invented house beyond the e2e one (house.ts): Tempo around today, Zigbee lamps,
// the AC, Garenne, the TV, the box and three scenes. readme.ts takes its picture; layout.ts
// measures the phone dashboard on it.
import type { Page, Route } from 'playwright';
import { broadlinkHost } from '../web/src/lib/test/broadlink.ts';
import { zigbeeLamp } from '../web/src/lib/test/lamps.ts';
import { forecastDay, tempoForecast, tempoStock, tempoToday } from '../web/src/lib/test/tempo.ts';
import type { TempoColor, TempoForecast, TempoToday } from '../web/src/lib/devices/tempo/api.ts';
import type { LampStats, ZigbeeLampsResponse } from '../web/src/lib/devices/lamps/api.ts';
import type { BroadlinkClimateStateResponse, BroadlinkDiscoverResponse } from '../web/src/lib/devices/climate/api.ts';
import type { NabaztagStatusResponse } from '../web/src/lib/devices/nabaztag/api.ts';
import type { AndroidTvStatusResponse, TvStatusResponse } from '../web/src/lib/devices/tv/api.ts';
import type { ScenesResponse } from '../web/src/lib/devices/scenes/api.ts';

/** Each route's answer, typed by the app's own types: a changed API fails to type-check here. */
interface Answers {
	'/api/tempo': TempoToday;
	'/api/tempo/forecast': TempoForecast;
	'/api/zigbee/lamps': ZigbeeLampsResponse;
	'/api/zigbee/lamps/stats': LampStats;
	'/api/broadlink/discover': BroadlinkDiscoverResponse;
	'/api/broadlink/mitsubishi/state': BroadlinkClimateStateResponse;
	'/api/nabaztag': NabaztagStatusResponse;
	'/api/tv': TvStatusResponse;
	'/api/scenes': ScenesResponse;
	'/api/androidtv': AndroidTvStatusResponse;
}

/** A local day `offset` days from today, « 2026-12-10 ». */
const day = (offset: number) => {
	const d = new Date();
	d.setDate(d.getDate() + offset);
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

/** What the dashboard shows beyond the e2e house: Tempo around today, Zigbee lamps, the AC,
 * Garenne, the TV and the box. Answers before the house (routes added last win). */
export async function showcase(page: Page) {
	const json = (route: Route, body: unknown) => route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });
	const outlook: [TempoColor, number][] = [
		['WHITE', 1],
		['RED', 0.81],
		['RED', 0.66],
		['WHITE', 0.58],
		['BLUE', 0.72],
		['BLUE', 0.84],
		['BLUE', 0.77]
	];
	const lamps = [
		zigbeeLamp({ id: 'zb-1', name: 'Pendant' }),
		zigbeeLamp({ id: 'zb-2', name: 'Bedside lamp', state: { isOn: false } }),
		zigbeeLamp({ id: 'zb-3', name: 'LED strip', state: { isOn: true, brightness: 35 } })
	];
	const answers: Answers = {
		'/api/tempo': tempoToday({
			yesterday: { date: day(-1), color: 'BLUE' },
			today: { date: day(0), color: 'WHITE' },
			tomorrow: { date: day(1), color: 'WHITE' },
			stock: tempoStock()
		}),
		'/api/tempo/forecast': tempoForecast({
			issued: day(0),
			weatherIssued: day(0),
			days: outlook.map(([color, p], i) => forecastDay(day(i + 1), i + 1, color, p, i === 0))
		}),
		'/api/zigbee/lamps': { success: true, lamps, total: lamps.length, connected: lamps.length, reachable: lamps.length, message: '' },
		'/api/zigbee/lamps/stats': { success: true, total: lamps.length, connected: lamps.length, reachable: lamps.length, disabled: false },
		'/api/broadlink/discover': { success: true, devices: [broadlinkHost()], total: 1, message: '' },
		'/api/broadlink/mitsubishi/state': {
			success: true,
			state: {
				power: true,
				lastCommand: 'state-heat-21-fan-auto-vane-auto',
				lastOnCommand: 'state-heat-21-fan-auto-vane-auto',
				settings: { mode: 'heat', temperature: 21, fan: 'auto', vane: 'auto', econo: false, stopInMinutes: null },
				host: broadlinkHost().host,
				model: 'msz-hj5va',
				updatedAt: new Date().toISOString()
			},
			message: ''
		},
		'/api/nabaztag': { success: true, config: { host: '192.168.1.50', tempoEnabled: true }, reachable: true },
		'/api/tv': {
			success: true,
			config: { host: '192.168.1.52', irBlasterHost: broadlinkHost().host, boxHost: '192.168.1.153' },
			status: {
				configured: true,
				power: 'standby',
				name: 'Living room TV',
				volume: { current: 12, min: 0, max: 60, muted: false },
				ambilight: { power: false }
			}
		},
		'/api/scenes': {
			success: true,
			scenes: [
				{ id: 'je-pars', name: 'Leaving', icon: 'log-out', actions: [{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' }] },
				{ id: 'nuit', name: 'Night', icon: 'moon', actions: [{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' }] },
				{ id: 'film', name: 'Movie', icon: 'film', actions: [{ action: 'tv_power', state: 'on' }] }
			]
		},
		'/api/androidtv': {
			success: true,
			config: { host: '192.168.1.153' },
			status: { configured: true, reachable: true, awake: false, model: 'LEAP-S1', paired: true }
		}
	};
	await page.route('**/api/**', (route) => {
		const { pathname } = new URL(route.request().url());
		return route.request().method() === 'GET' && pathname in answers ? json(route, answers[pathname as keyof Answers]) : route.fallback();
	});
}
