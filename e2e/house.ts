// A small simulated house, served by Playwright in place of the devices: the real backend
// keeps auth, Tempo and everything without hardware; the device endpoints below answer from
// this state, which the scenario can read back and change (a lamp that stops answering). Every
// answer is typed with the app's own types and built from the unit tests' fixtures
// (web/src/lib/test), so a changed API fails to type-check here too (`bun run check`).
import type { Page, Route } from 'playwright';
import type {
	DeviceStatusResponse,
	DevicesResponse,
	FountainStatus,
	LitterBoxStatus,
	MealPlanEntry,
	MealPlanResponse
} from '../web/src/lib/devices/cats/api.ts';
import type { HueLampActionResponse, HueLampsResponse, HueLampStatusResponse, LampStats } from '../web/src/lib/devices/lamps/api.ts';
import type {
	MerossConsumptionResponse,
	MerossPlugsResponse,
	MerossPlugStatusResponse,
	MerossToggleResponse
} from '../web/src/lib/devices/meross/api.ts';
import type { ShutterResponse, ShuttersResponse } from '../web/src/lib/devices/shutters/api.ts';
import type { BroadlinkDiscoverResponse } from '../web/src/lib/devices/climate/api.ts';
import type { SimpleResponse } from '../web/src/lib/api.ts';
import { hueLamp } from '../web/src/lib/test/lamps.ts';
import { merossElectricity, merossPlug, merossStatus } from '../web/src/lib/test/meross.ts';
import { shutter } from '../web/src/lib/test/shutters.ts';
import { feederStatus, tuyaDevice } from '../web/src/lib/test/tuya.ts';
import { broadlinkHost } from '../web/src/lib/test/broadlink.ts';

/** Today at 23:30, the sun schedule's next closing (ISO). */
function tonight(): string {
	const d = new Date();
	d.setHours(23, 30, 0, 0);
	return d.toISOString();
}

/** How long the feeder takes to save a plan: long enough to see a switch stay busy. */
const FEEDER_SAVE_MS = 300;

export function house() {
	const state = {
		hue: [
			hueLamp({
				id: 'hue-01',
				name: 'Salon',
				state: { isOn: true, brightness: 80, temperature: null, temperatureMin: null, temperatureMax: null }
			}),
			hueLamp({
				id: 'hue-02',
				name: 'Chambre',
				connected: false,
				reachable: false,
				lastSeen: new Date(Date.now() - 12 * 60_000).toISOString(),
				state: { isOn: false, temperature: null, temperatureMin: null, temperatureMax: null }
			})
		],
		plugs: [
			merossPlug({ id: 'plug-1', name: 'Lave-linge', ip: '192.168.1.60', isOnline: true, isOn: true }),
			merossPlug({ id: 'plug-2', name: 'Radiateur', ip: '192.168.1.61', isOnline: false, isOn: false })
		],
		covers: [
			shutter({
				id: '0000000000000002',
				name: 'Volet salon',
				openPercent: 100,
				targetOpenPercent: 100,
				nextClose: tonight(),
				schedule: { openAtSunrise: true, closeAtSunset: true, sunriseOffsetMin: 0, sunsetOffsetMin: 60 }
			})
		],
		tuya: [
			tuyaDevice({ id: 'feeder-1', name: 'Pixi Feeder', productName: 'Pixi Smart Feeder', ip: '192.168.1.174' }),
			tuyaDevice({ id: 'fountain-1', name: 'Fontaine', type: 'fountain', ip: '192.168.1.175' }),
			tuyaDevice({ id: 'litter-1', name: 'Litière', type: 'litter-box', ip: '192.168.1.176', connected: false })
		],
		/** What the fountain and the litter box report. */
		fountain: { waterLevel: 'low', power: true, filterLife: 2400 } as FountainStatus,
		litter: { sensors: { litterLevel: 'half' }, system: { state: 'satnd_by' } } as LitterBoxStatus,
		meals: [
			{
				daysOfWeek: ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'],
				time: '08:00',
				portion: 2,
				status: 'Enabled'
			}
		] as MealPlanEntry[],
		/** Every command the page sent, in order: « POST /api/hue-lamps/hue-01/power {"enabled":false} ». */
		sent: [] as string[],
		/** Every read the page made, in order: « /api/meross ». */
		asked: [] as string[],
		/** A device family that stops answering commands (to see « Pas de réponse »). */
		silent: new Set<string>()
	};

	/** One answer, typed by the route that sends it. */
	const json = <T>(route: Route, body: T) => route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });

	async function serve(page: Page) {
		await page.route('**/api/**', async (route) => {
			const req = route.request();
			const { pathname } = new URL(req.url());
			const method = req.method();
			const body = req.postDataJSON?.() ?? null;
			if (method === 'GET') state.asked.push(pathname);
			else state.sent.push(`${method} ${pathname}${body ? ' ' + JSON.stringify(body) : ''}`);

			let m: RegExpMatchArray | null;
			if (method === 'GET' && pathname === '/api/hue-lamps') {
				return json<HueLampsResponse>(route, {
					success: true,
					lamps: state.hue,
					total: state.hue.length,
					connected: state.hue.filter((l) => l.connected).length,
					reachable: state.hue.filter((l) => l.reachable).length,
					message: ''
				});
			}
			if (method === 'GET' && pathname === '/api/hue-lamps/stats') {
				return json<LampStats>(route, { success: true, total: state.hue.length, connected: 1, reachable: 1, disabled: false });
			}
			if ((m = pathname.match(/^\/api\/hue-lamps\/([^/]+)\/power$/)) && method === 'POST') {
				if (state.silent.has('hue')) return; // never answers: the page must give up on its own
				const lamp = state.hue.find((l) => l.id === m![1])!;
				lamp.state.isOn = body.enabled ?? !lamp.state.isOn;
				return json<HueLampActionResponse>(route, { success: true, state: { isOn: lamp.state.isOn, brightness: lamp.state.brightness } });
			}
			if ((m = pathname.match(/^\/api\/hue-lamps\/([^/]+)\/brightness$/)) && method === 'POST') {
				const lamp = state.hue.find((l) => l.id === m![1])!;
				lamp.state.brightness = body.brightness;
				return json<HueLampActionResponse>(route, { success: true, state: { isOn: lamp.state.isOn, brightness: lamp.state.brightness } });
			}
			if (method === 'GET' && pathname === '/api/meross') {
				return json<MerossPlugsResponse>(route, { success: true, devices: state.plugs, total: state.plugs.length, message: '' });
			}
			if ((m = pathname.match(/^\/api\/meross\/([^/]+)\/toggle$/)) && method === 'POST') {
				const plug = state.plugs.find((p) => p.id === m![1])!;
				plug.isOn = body.on;
				return json<MerossToggleResponse>(route, { success: true, device: { id: plug.id, name: plug.name }, on: plug.isOn, message: '' });
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/meross\/([^/]+)\/electricity$/))) {
				return json(route, merossElectricity(m[1]));
			}
			if (method === 'GET' && pathname === '/api/matter/covers') {
				return json<ShuttersResponse>(route, { success: true, covers: state.covers });
			}
			if ((m = pathname.match(/^\/api\/matter\/covers\/([^/]+)\/skip$/)) && method === 'POST') {
				const cover = state.covers.find((c) => c.id === m![1])!;
				if (body.event === 'open') cover.skipNextOpen = body.skip;
				else cover.skipNextClose = body.skip;
				return json<ShutterResponse>(route, { success: true, cover });
			}
			if ((m = pathname.match(/^\/api\/matter\/covers\/([^/]+)\/(open|close|stop|position)$/)) && method === 'POST') {
				const cover = state.covers.find((c) => c.id === m![1])!;
				const to = m[2] === 'open' ? 100 : m[2] === 'close' ? 0 : m[2] === 'position' ? body.openPercent : cover.openPercent;
				cover.openPercent = cover.targetOpenPercent = to;
				return json<ShutterResponse>(route, { success: true, cover });
			}
			if (method === 'GET' && pathname === '/api/devices') {
				return json<DevicesResponse>(route, { success: true, devices: state.tuya, total: state.tuya.length, message: '' });
			}
			// the detail pages
			if (method === 'GET' && (m = pathname.match(/^\/api\/hue-lamps\/([^/]+)$/))) {
				return json<HueLampStatusResponse>(route, { success: true, lamp: state.hue.find((l) => l.id === m![1]) });
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/meross\/([^/]+)\/status$/))) {
				const plug = state.plugs.find((p) => p.id === m![1])!;
				return json<MerossPlugStatusResponse>(route, {
					...merossStatus({ online: plug.isOnline, on: plug.isOn, electricity: { voltage: 230.4, current: 0.183, power: 42.4 } }, plug.id),
					device: { id: plug.id, name: plug.name }
				});
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/meross\/([^/]+)\/consumption$/))) {
				const plug = state.plugs.find((p) => p.id === m![1])!;
				return json<MerossConsumptionResponse>(route, {
					success: true,
					device: { id: plug.id, name: plug.name },
					consumption: [
						{ date: '2026-10-01', time: 0, value: 812 },
						{ date: '2026-10-02', time: 0, value: 655 }
					],
					summary: { days: 2, totalWh: 1467, totalKwh: 1.467 },
					message: ''
				});
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/devices\/([^/]+)\/(feeder|fountain|litter-box)\/status$/))) {
				const d = state.tuya.find((x) => x.id === m![1])!;
				const parsedStatus = m[2] === 'feeder' ? feederStatus() : m[2] === 'fountain' ? state.fountain : state.litter;
				return json<DeviceStatusResponse>(route, {
					success: true,
					device: { id: d.id, name: d.name, type: d.type, connected: d.connected },
					parsedStatus,
					message: ''
				});
			}
			if (method === 'POST' && /^\/api\/devices\/[^/]+\/feeder\/meal-plan$/.test(pathname)) {
				state.meals = body.mealPlan;
				await new Promise((r) => setTimeout(r, FEEDER_SAVE_MS));
				return json<SimpleResponse>(route, { success: true, message: '' });
			}
			if (method === 'GET' && /^\/api\/devices\/[^/]+\/feeder\/meal-plan$/.test(pathname)) {
				return json<MealPlanResponse>(route, {
					success: true,
					device: { id: 'feeder-1', name: 'Pixi Feeder' },
					decoded: state.meals,
					mealPlan: null,
					message: ''
				});
			}
			// the IR blaster: the backend would broadcast a discovery on the real LAN; answer with
			// a blaster at a TEST-NET address, which the backend refuses before any frame leaves
			if (method === 'GET' && pathname === '/api/broadlink/discover') {
				return json<BroadlinkDiscoverResponse>(route, { success: true, devices: [broadlinkHost()], total: 1, message: '' });
			}
			return route.fallback(); // the real backend
		});
	}

	return { state, serve };
}
