// Not a check: the README's picture. The dashboard of an invented, well-stocked house (the
// e2e house, plus every family the dashboard shows), signed in like any scenario: nothing of
// the real flat, no login to keep alive, the same image every time. scripts/screenshot.sh
// runs it and passes OUT (the JPEG to write).
import type { Page, Route } from 'playwright';
import { BASE, done, launch, open, signIn } from './lib.ts';
import { house } from './house.ts';
import { broadlinkHost } from '../web/src/lib/test/remote.ts';
import { zigbeeLamp } from '../web/src/lib/test/lamps.ts';
import { forecastDay, tempoForecast, tempoStock, tempoToday } from '../web/src/lib/test/tempo.ts';
import type { TempoColor } from '../web/src/lib/api.ts';

const OUT = process.env.OUT ?? 'shots/readme.jpg';

/** A local day `offset` days from today, « 2026-12-10 ». */
const day = (offset: number) => {
	const d = new Date();
	d.setDate(d.getDate() + offset);
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

/** What the dashboard shows beyond the e2e house: Tempo around today, Zigbee lamps, the AC,
 * Garenne, the TV and the box. Answers before the house (routes added last win). */
async function showcase(page: Page) {
	const json = (route: Route, body: unknown) => route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });
	const outlook: [TempoColor, number][] = [['WHITE', 1], ['RED', 0.81], ['RED', 0.66], ['WHITE', 0.58], ['BLUE', 0.72], ['BLUE', 0.84], ['BLUE', 0.77]];
	const lamps = [
		zigbeeLamp({ id: 'zb-1', name: 'Suspension' }),
		zigbeeLamp({ id: 'zb-2', name: 'Lampe de chevet', state: { isOn: false } }),
		zigbeeLamp({ id: 'zb-3', name: 'Ruban LED', state: { isOn: true, brightness: 35 } })
	];
	const answers: Record<string, unknown> = {
		'/api/tempo': tempoToday({
			yesterday: { date: day(-1), color: 'BLUE' },
			today: { date: day(0), color: 'WHITE' },
			tomorrow: { date: day(1), color: 'WHITE' },
			stock: tempoStock()
		}),
		'/api/tempo/forecast': tempoForecast({
			issued: day(0),
			weather_issued: day(0),
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
			}
		},
		'/api/nabaztag': { success: true, config: { host: '192.168.1.50', tempoEnabled: true }, reachable: true },
		'/api/tv': {
			success: true,
			config: { host: '192.168.1.52', irBlasterHost: broadlinkHost().host, boxHost: '192.168.1.153' },
			status: { configured: true, power: 'standby', name: 'TV du salon', volume: { current: 12, min: 0, max: 60, muted: false }, ambilight: { power: false } }
		},
		'/api/androidtv': {
			success: true,
			config: { host: '192.168.1.153' },
			status: { configured: true, reachable: true, awake: false, currentApp: null, model: 'LEAP-S1', paired: true }
		}
	};
	await page.route('**/api/**', (route) => {
		const { pathname } = new URL(route.request().url());
		return route.request().method() === 'GET' && pathname in answers ? json(route, answers[pathname]) : route.fallback();
	});
}

const browser = await launch();
const p = await open(browser, { colorScheme: 'light', viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
await house().serve(p);
await showcase(p);
await signIn(p);
await p.goto(BASE + '/');
await p.locator('main h1').first().waitFor();
// every group has answered (no skeleton left), then the fonts and the last paint
await p.waitForFunction(() => !document.querySelector('[aria-busy="true"], .skeleton'), undefined, { timeout: 15_000 }).catch(() => {});
await p.evaluate(() => document.fonts.ready);
await p.waitForTimeout(800);
await p.screenshot({ path: OUT, fullPage: true, type: 'jpeg', quality: 90 });
console.log(`Saved -> ${OUT}`);
await done(browser);
