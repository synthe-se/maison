// A small simulated house, served by Playwright in place of the devices: the real backend
// keeps auth, Tempo and everything without hardware; the device endpoints below answer from
// this state, which the scenario can read back and change (a lamp that stops answering). The
// shapes are the app's own types, so a changed API fails to type-check here too.
import type { Page, Route } from 'playwright';
import type { DevicesResponse, HueLampsResponse, MealPlanEntry, MerossPlugsResponse, ShuttersResponse } from '../web/src/lib/api.ts';
// the same fixtures as the unit tests: one description of what the backend sends
import { hueLamp } from '../web/src/lib/test/lamps.ts';
import { merossElectricity, merossPlug, merossStatus } from '../web/src/lib/test/meross.ts';
import { shutter } from '../web/src/lib/test/shutters.ts';
import { tuyaDevice } from '../web/src/lib/test/tuya.ts';

export function house() {
	const state = {
		hue: [
			hueLamp({ id: 'hue-01', name: 'Salon', state: { isOn: true, brightness: 80, temperature: null, temperatureMin: null, temperatureMax: null } }),
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
		covers: [shutter({ id: '0000000000000002', name: 'Volet salon', openPercent: 100, targetOpenPercent: 100 })],
		tuya: [tuyaDevice({ id: 'feeder-1', name: 'Pixi Feeder', product_name: 'Pixi Smart Feeder', ip: '192.168.1.174' })],
		meals: [{ days_of_week: ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'], time: '08:00', portion: 2, status: 'Enabled' }] as MealPlanEntry[],
		/** Every command the page sent, in order: « POST /api/hue-lamps/hue-01/power {"enabled":false} ». */
		sent: [] as string[],
		/** A device family that stops answering commands (to see « Pas de réponse »). */
		silent: new Set<string>()
	};

	const json = (route: Route, body: unknown) => route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });

	async function serve(page: Page) {
		await page.route('**/api/**', async (route) => {
			const req = route.request();
			const { pathname } = new URL(req.url());
			const method = req.method();
			const body = req.postDataJSON?.() ?? null;
			if (method !== 'GET') state.sent.push(`${method} ${pathname}${body ? ' ' + JSON.stringify(body) : ''}`);

			let m: RegExpMatchArray | null;
			if (method === 'GET' && pathname === '/api/hue-lamps') {
				const r: HueLampsResponse = {
					success: true,
					lamps: state.hue,
					total: state.hue.length,
					connected: state.hue.filter((l) => l.connected).length,
					reachable: state.hue.filter((l) => l.reachable).length,
					message: ''
				};
				return json(route, r);
			}
			if (method === 'GET' && pathname === '/api/hue-lamps/stats') {
				return json(route, { success: true, total: state.hue.length, connected: 1, reachable: 1, disabled: false });
			}
			if ((m = pathname.match(/^\/api\/hue-lamps\/([^/]+)\/power$/)) && method === 'POST') {
				if (state.silent.has('hue')) return; // never answers: the page must give up on its own
				const lamp = state.hue.find((l) => l.id === m![1])!;
				lamp.state.isOn = body.enabled ?? body.on ?? !lamp.state.isOn;
				return json(route, { success: true, state: { isOn: lamp.state.isOn, brightness: lamp.state.brightness } });
			}
			if ((m = pathname.match(/^\/api\/hue-lamps\/([^/]+)\/brightness$/)) && method === 'POST') {
				const lamp = state.hue.find((l) => l.id === m![1])!;
				lamp.state.brightness = body.brightness;
				return json(route, { success: true, state: { isOn: lamp.state.isOn, brightness: lamp.state.brightness } });
			}
			if (method === 'GET' && pathname === '/api/meross') {
				const r: MerossPlugsResponse = { success: true, devices: state.plugs, total: state.plugs.length, message: '' };
				return json(route, r);
			}
			if ((m = pathname.match(/^\/api\/meross\/([^/]+)\/(toggle|on|off)$/)) && method === 'POST') {
				const plug = state.plugs.find((p) => p.id === m![1])!;
				plug.isOn = m[2] === 'toggle' ? body.on : m[2] === 'on';
				return json(route, { success: true, device: { id: plug.id, name: plug.name }, on: plug.isOn, message: '' });
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/meross\/([^/]+)\/electricity$/))) {
				return json(route, merossElectricity(m[1]));
			}
			if (method === 'GET' && pathname === '/api/matter/covers') {
				const r: ShuttersResponse = { success: true, covers: state.covers };
				return json(route, r);
			}
			if ((m = pathname.match(/^\/api\/matter\/covers\/([^/]+)\/(open|close|stop|position)$/)) && method === 'POST') {
				const cover = state.covers.find((c) => c.id === m![1])!;
				const to = m[2] === 'open' ? 100 : m[2] === 'close' ? 0 : m[2] === 'position' ? body.openPercent : cover.openPercent;
				cover.openPercent = cover.targetOpenPercent = to;
				return json(route, { success: true, cover });
			}
			if (method === 'GET' && pathname === '/api/devices') {
				const r: DevicesResponse = { success: true, devices: state.tuya, total: state.tuya.length, message: '' };
				return json(route, r);
			}
			// the detail pages
			if (method === 'GET' && (m = pathname.match(/^\/api\/hue-lamps\/([^/]+)$/))) {
				return json(route, { success: true, lamp: state.hue.find((l) => l.id === m![1]) });
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/meross\/([^/]+)\/status$/))) {
				const plug = state.plugs.find((p) => p.id === m![1])!;
				return json(route, { ...merossStatus({ online: plug.isOnline, on: plug.isOn, electricity: { voltage: 230.4, current: 0.183, power: 42.4 } }, plug.id), device: { id: plug.id, name: plug.name } });
			}
			if (method === 'GET' && /^\/api\/meross\/[^/]+\/consumption$/.test(pathname)) {
				return json(route, { success: true, device: { id: 'x', name: 'x' }, consumption: [{ date: '2026-10-01', time: 0, value: 812 }, { date: '2026-10-02', time: 0, value: 655 }], summary: { days: 2, totalWh: 1467, totalKwh: 1.467 }, message: '' });
			}
			if (method === 'GET' && (m = pathname.match(/^\/api\/devices\/([^/]+)\/(feeder\/)?status$/))) {
				return json(route, {
					success: true,
					device: { id: m[1], name: 'Pixi Feeder', type: 'feeder', connected: true },
					parsed_status: { food_level: 'full', battery_level: 76, portions_today: 3, fault: false, power_mode: 'usb' },
					message: ''
				});
			}
			if (method === 'POST' && /^\/api\/devices\/[^/]+\/feeder\/meal-plan$/.test(pathname)) {
				state.meals = body.meal_plan;
				// a feeder takes a moment: long enough to see the switch stay busy, and focused
				await new Promise((r) => setTimeout(r, 300));
				return json(route, { success: true, message: '' });
			}
			if (method === 'GET' && /^\/api\/devices\/[^/]+\/feeder\/meal-plan$/.test(pathname)) {
				return json(route, { success: true, device: { id: 'feeder-1', name: 'Pixi Feeder' }, decoded: state.meals, meal_plan: null, message: '' });
			}
			return route.fallback(); // the real backend
		});
	}

	return { state, serve };
}
