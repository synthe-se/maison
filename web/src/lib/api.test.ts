import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { json as jsonBody, scriptFetch as script, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { ApiError, UNREACHABLE, api, onUnauthorized, path, toApiError } from './api.ts';
import { authApi, peopleApi } from './passkeys.ts';
import { devicesApi, feederApi, fountainApi, litterBoxApi } from './devices/cats/api.ts';
import { hueLampsApi, zigbeeLampsApi } from './devices/lamps/api.ts';
import { merossApi } from './devices/meross/api.ts';
import { broadlinkApi } from './devices/climate/api.ts';
import { tempoApi } from './devices/tempo/api.ts';
import { irApi } from './devices/remote/api.ts';
import { nabaztagApi } from './devices/nabaztag/api.ts';
import { androidTvApi, tvApi } from './devices/tv/api.ts';
import { shuttersApi } from './devices/shutters/api.ts';

const json = (status: number, body: unknown) => jsonBody(body, status);

describe('api', () => {
	beforeEach(() => onUnauthorized(() => {}));
	afterEach(() => vi.unstubAllGlobals());

	it('sends JSON with the session cookie and parses the answer', async () => {
		const calls = script(json(200, { success: true, n: 1 }));
		const r = await api<{ n: number }>('/thing', { method: 'POST', body: { a: 1 } });
		expect(r.n).toBe(1);
		expect(calls[0].url).toBe('/api/thing');
		expect(calls[0].init?.method).toBe('POST');
		expect(calls[0].init?.body).toBe('{"a":1}');
		expect(calls[0].init?.credentials).toBe('same-origin');
	});

	it('refreshes the session once on 401, then retries the request', async () => {
		const calls = script(json(401, {}), json(200, {}), json(200, { ok: 'again' }));
		const r = await api<{ ok: string }>('/lamps');
		expect(r.ok).toBe('again');
		expect(calls.map((c) => c.url)).toEqual(['/api/lamps', '/api/auth/refresh', '/api/lamps']);
	});

	it('shares one refresh between requests refused at the same time', async () => {
		const calls = script(json(401, {}), json(401, {}), json(200, {}), json(200, { a: 1 }), json(200, { b: 2 }));
		await Promise.all([api('/a'), api('/b')]);
		expect(calls.filter((c) => c.url === '/api/auth/refresh')).toHaveLength(1);
	});

	it('tells the session it is over when the refresh fails too', async () => {
		const over = vi.fn();
		onUnauthorized(over);
		// the refresh is refused too: no retry, the original refusal is the error
		script(json(401, { error: 'expired' }), json(401, {}));
		await expect(api('/lamps')).rejects.toThrow('expired');
		expect(over).toHaveBeenCalledOnce();
	});

	it('does not sign out over a brief outage: a renewal answered 502 is « unreachable »', async () => {
		const over = vi.fn();
		onUnauthorized(over);
		script(json(401, {}), new Response('<html>', { status: 502 }));
		await expect(api('/lamps')).rejects.toMatchObject({ code: UNREACHABLE, status: 502 });
		expect(over).not.toHaveBeenCalled();
	});

	it('does not sign out when the renewal gets no answer at all', async () => {
		const over = vi.fn();
		onUnauthorized(over);
		script(json(401, {}), new TypeError('Failed to fetch'));
		await expect(api('/lamps')).rejects.toMatchObject({ code: UNREACHABLE });
		expect(over).not.toHaveBeenCalled();
	});

	it('a renewal refused with 403 ends the session too', async () => {
		const over = vi.fn();
		onUnauthorized(over);
		script(json(401, {}), json(403, {}));
		await expect(api('/lamps')).rejects.toMatchObject({ status: 401 });
		expect(over).toHaveBeenCalledOnce();
	});

	it('does not end the session for a refused session check', async () => {
		const over = vi.fn();
		onUnauthorized(over);
		script(json(401, { error: 'Invalid token' }), json(401, {}));
		await expect(api('/auth/verify', { method: 'POST' })).rejects.toThrow('Invalid token');
		expect(over).not.toHaveBeenCalled();
	});

	it('keeps the name of a refusal, for the app to word it', async () => {
		script(json(409, { success: false, error: 'The last passkey stays', code: 'last_passkey' }));
		await expect(api('/passkeys/k1', { method: 'DELETE' })).rejects.toMatchObject({ status: 409, code: 'last_passkey' });
	});

	it('uses the server’s error message, or leaves it empty for errors.ts to word the status', async () => {
		script(json(503, { success: false, error: 'TV unreachable' }));
		await expect(api('/tv')).rejects.toThrow('TV unreachable');
		script(new Response('<html>', { status: 502 }));
		await expect(api('/tv')).rejects.toMatchObject({ status: 502, message: '' });
	});

	it('sends no JSON content type without a body', async () => {
		const calls = script(json(200, {}));
		await api('/x', { method: 'POST' });
		expect(calls[0].init?.body).toBeUndefined();
		expect(new Headers(calls[0].init?.headers).has('content-type')).toBe(false);
	});

	it('accepts an empty body', async () => {
		script(new Response(null, { status: 204 }));
		await expect(api('/x', { method: 'DELETE' })).resolves.toEqual({});
	});
});

/** Each wrapper: the request the backend receives (method, path, JSON body). */
const endpoints: [string, () => Promise<unknown>, string, unknown?][] = [
	['auth.verify', () => authApi.verify(), 'POST /auth/verify'],
	['auth.logout', () => authApi.logout(), 'POST /auth/logout'],
	['auth.logoutEverywhere', () => authApi.logoutEverywhere(), 'POST /auth/logout-everywhere'],
	['devices.list', () => devicesApi.list(), 'GET /devices'],
	['devices.connect', () => devicesApi.connect('d1'), 'POST /devices/d1/connect'],
	['devices.connectAll', () => devicesApi.connectAll(), 'POST /devices/connect'],
	['devices.disconnect', () => devicesApi.disconnect('d1'), 'POST /devices/d1/disconnect'],
	['devices.disconnectAll', () => devicesApi.disconnectAll(), 'POST /devices/disconnect'],
	['feeder.status', () => feederApi.status('d1'), 'GET /devices/d1/feeder/status'],
	['feeder.feed (one portion by default)', () => feederApi.feed('d1'), 'POST /devices/d1/feeder/feed', { portion: 1 }],
	['feeder.getMealPlan', () => feederApi.getMealPlan('d1'), 'GET /devices/d1/feeder/meal-plan'],
	['feeder.setMealPlan', () => feederApi.setMealPlan('d1', []), 'POST /devices/d1/feeder/meal-plan', { mealPlan: [] }],
	['fountain.status', () => fountainApi.status('d1'), 'GET /devices/d1/fountain/status'],
	['fountain.power', () => fountainApi.power('d1', true), 'POST /devices/d1/fountain/power', { enabled: true }],
	['fountain.resetWater', () => fountainApi.resetWater('d1'), 'POST /devices/d1/fountain/reset/water'],
	['fountain.resetFilter', () => fountainApi.resetFilter('d1'), 'POST /devices/d1/fountain/reset/filter'],
	['fountain.resetPump', () => fountainApi.resetPump('d1'), 'POST /devices/d1/fountain/reset/pump'],
	['fountain.setUV', () => fountainApi.setUV('d1', false), 'POST /devices/d1/fountain/uv', { enabled: false }],
	['fountain.setEcoMode', () => fountainApi.setEcoMode('d1', 2), 'POST /devices/d1/fountain/eco-mode', { mode: 2 }],
	['litterBox.status', () => litterBoxApi.status('d1'), 'GET /devices/d1/litter-box/status'],
	['litterBox.clean', () => litterBoxApi.clean('d1'), 'POST /devices/d1/litter-box/clean'],
	['litterBox.settings', () => litterBoxApi.settings('d1', { cleanDelay: 5 }), 'POST /devices/d1/litter-box/settings', { cleanDelay: 5 }],
	['hue.list', () => hueLampsApi.list(), 'GET /hue-lamps'],
	['hue.scan', () => hueLampsApi.scan(), 'POST /hue-lamps/scan'],
	['hue.stats', () => hueLampsApi.stats(), 'GET /hue-lamps/stats'],
	['hue.status', () => hueLampsApi.status('l1'), 'GET /hue-lamps/l1'],
	['hue.power', () => hueLampsApi.power('l1', true), 'POST /hue-lamps/l1/power', { enabled: true }],
	['hue.brightness', () => hueLampsApi.brightness('l1', 80), 'POST /hue-lamps/l1/brightness', { brightness: 80 }],
	['hue.temperature', () => hueLampsApi.temperature('l1', 300), 'POST /hue-lamps/l1/temperature', { temperature: 300 }],
	['hue.blacklist', () => hueLampsApi.blacklist('l1'), 'POST /hue-lamps/l1/blacklist'],
	['zigbee.list', () => zigbeeLampsApi.list(), 'GET /zigbee/lamps'],
	['zigbee.stats', () => zigbeeLampsApi.stats(), 'GET /zigbee/lamps/stats'],
	['zigbee.status', () => zigbeeLampsApi.status('z1'), 'GET /zigbee/lamps/z1'],
	['zigbee.pairingStatus', () => zigbeeLampsApi.pairingStatus(), 'GET /zigbee/lamps/pairing/status'],
	['zigbee.startPairing', () => zigbeeLampsApi.startPairing(), 'POST /zigbee/lamps/pairing/start'],
	['zigbee.stopPairing', () => zigbeeLampsApi.stopPairing(), 'POST /zigbee/lamps/pairing/stop'],
	['zigbee.touchlinkScan', () => zigbeeLampsApi.touchlinkScan(), 'POST /zigbee/lamps/pairing/touchlink'],
	['zigbee.power', () => zigbeeLampsApi.power('z1', false), 'POST /zigbee/lamps/z1/power', { enabled: false }],
	['zigbee.brightness', () => zigbeeLampsApi.brightness('z1', 10), 'POST /zigbee/lamps/z1/brightness', { brightness: 10 }],
	['zigbee.temperature', () => zigbeeLampsApi.temperature('z1', 250), 'POST /zigbee/lamps/z1/temperature', { temperature: 250 }],
	['zigbee.color', () => zigbeeLampsApi.color('z1', 0.3, 0.4), 'POST /zigbee/lamps/z1/color', { x: 0.3, y: 0.4 }],
	['zigbee.effect', () => zigbeeLampsApi.effect('z1', 'blink'), 'POST /zigbee/lamps/z1/effect', { effect: 'blink' }],
	['zigbee.rename', () => zigbeeLampsApi.rename('z1', 'Cuisine'), 'POST /zigbee/lamps/z1/rename', { name: 'Cuisine' }],
	['meross.list', () => merossApi.list(), 'GET /meross'],
	['meross.status', () => merossApi.status('p1'), 'GET /meross/p1/status'],
	['meross.electricity', () => merossApi.electricity('p1'), 'GET /meross/p1/electricity'],
	['meross.toggle', () => merossApi.toggle('p1', true), 'POST /meross/p1/toggle', { on: true }],
	['meross.consumption', () => merossApi.consumption('p1'), 'GET /meross/p1/consumption'],
	['meross.dnd', () => merossApi.dnd('p1', true), 'POST /meross/p1/dnd', { enabled: true }],
	['broadlink.discover', () => broadlinkApi.discover(), 'GET /broadlink/discover'],
	['broadlink.discover (a new scan)', () => broadlinkApi.discover(true), 'GET /broadlink/discover?forceRefresh=true'],
	['broadlink.listCodes', () => broadlinkApi.listCodes(), 'GET /broadlink/codes'],
	['broadlink.getMitsubishiState', () => broadlinkApi.getMitsubishiState(), 'GET /broadlink/mitsubishi/state'],
	[
		'broadlink.sendMitsubishiCommand',
		() => broadlinkApi.sendMitsubishiCommand('h', 'heat_21'),
		'POST /broadlink/mitsubishi/send',
		{ host: 'h', command: 'heat_21' }
	],
	['tempo.get', () => tempoApi.get(), 'GET /tempo'],
	['tempo.forecast', () => tempoApi.forecast(), 'GET /tempo/forecast'],
	['tempo.calendar', () => tempoApi.calendar(), 'GET /tempo/calendar'],
	['tempo.calendar (a season)', () => tempoApi.calendar('2025-2026'), 'GET /tempo/calendar?season=2025-2026'],
	['ir.keymap', () => irApi.keymap(), 'GET /ir/keymap'],
	['ir.removeBinding', () => irApi.removeBinding(12), 'DELETE /ir/keymap/12'],
	['ir.recent', () => irApi.recent(), 'GET /ir/recent'],
	['ir.test', () => irApi.test([]), 'POST /ir/test', { actions: [] }],
	['nabaztag.status', () => nabaztagApi.status(), 'GET /nabaztag'],
	['people.list', () => peopleApi.list(), 'GET /people'],
	['people.remove', () => peopleApi.remove('alex'), 'DELETE /people/alex'],
	['nabaztag.pushTempo (cached by default)', () => nabaztagApi.pushTempo(), 'POST /nabaztag/tempo/push', { forceRefresh: false }],
	['tv.status', () => tvApi.status(), 'GET /tv'],
	['tv.power (to the box by default)', () => tvApi.power('on'), 'POST /tv/power', { state: 'on', switchToBox: true }],
	['tv.setVolume', () => tvApi.setVolume(12, false), 'PUT /tv/volume', { level: 12, muted: false }],
	['tv.ambilight', () => tvApi.ambilight('toggle'), 'POST /tv/ambilight', { state: 'toggle' }],
	['tv.switchToBox', () => tvApi.switchToBox(), 'POST /tv/source/box'],
	['shutters.list', () => shuttersApi.list(), 'GET /matter/covers'],
	[
		'shutters.commission',
		() => shuttersApi.commission('3497-011-2332', 'Salon'),
		'POST /matter/commission',
		{ code: '3497-011-2332', name: 'Salon' }
	],
	['shutters.open', () => shuttersApi.open('c1'), 'POST /matter/covers/c1/open'],
	['shutters.close', () => shuttersApi.close('c1'), 'POST /matter/covers/c1/close'],
	['shutters.stop', () => shuttersApi.stop('c1'), 'POST /matter/covers/c1/stop'],
	['shutters.setPosition', () => shuttersApi.setPosition('c1', 40), 'POST /matter/covers/c1/position', { openPercent: 40 }],
	['shutters.rename', () => shuttersApi.rename('c1', 'Chambre'), 'PATCH /matter/covers/c1', { name: 'Chambre' }],
	['shutters.remove', () => shuttersApi.remove('c1'), 'DELETE /matter/covers/c1'],
	['androidTv.status', () => androidTvApi.status(), 'GET /androidtv'],
	['androidTv.setConfig', () => androidTvApi.setConfig({ host: 'box' }), 'PUT /androidtv/config', { host: 'box' }],
	['androidTv.sendKey', () => androidTvApi.sendKey('ok'), 'POST /androidtv/key', { key: 'ok' }],
	[
		'androidTv.launch (waking the TV by default)',
		() => androidTvApi.launch('com.netflix'),
		'POST /androidtv/launch',
		{ package: 'com.netflix', ensureTvOn: true }
	],
	['androidTv.apps', () => androidTvApi.apps(), 'GET /androidtv/apps'],
	['androidTv.wake', () => androidTvApi.wake(), 'POST /androidtv/wake'],
	['androidTv.sleep', () => androidTvApi.sleep(), 'POST /androidtv/sleep'],
	['androidTv.pairStart', () => androidTvApi.pairStart(), 'POST /androidtv/pair/start'],
	['androidTv.pairFinish', () => androidTvApi.pairFinish('A1B2C3'), 'POST /androidtv/pair/finish', { code: 'A1B2C3' }]
];

describe('toApiError', () => {
	it('keeps the server’s words, its refusal’s name and what goes with it', () => {
		const e = toApiError(409, { success: false, error: 'Taken', code: 'person_exists', detail: { person: { id: 'leonard' } } });
		expect(e).toBeInstanceOf(ApiError);
		expect([e.message, e.status, e.code, e.detail]).toEqual(['Taken', 409, 'person_exists', { person: { id: 'leonard' } }]);
	});

	it('nothing to read (a proxy’s page, an empty body): the status alone', () => {
		for (const body of [undefined, null, 'oops', { error: 42, code: {} }]) {
			const e = toApiError(502, body);
			expect([e.message, e.status, e.code, e.detail]).toEqual(['', 502, undefined, undefined]);
		}
	});
});

describe('path', () => {
	it('encodes every interpolated value, so an id never breaks out of its segment', () => {
		expect(path`/meross/${'a/b?c'}/status`).toBe('/meross/a%2Fb%3Fc/status');
		expect(path`/ir/keymap/${12}`).toBe('/ir/keymap/12');
	});

	it('is used by the wrappers', async () => {
		const calls = stubFetch(() => jsonBody({ success: true }));
		await merossApi.status('../auth');
		expect(calls[0].url).toBe('/api/meross/..%2Fauth/status');
	});
});

describe('endpoints', () => {
	afterEach(() => vi.unstubAllGlobals());

	it.each(endpoints)('%s → %s', async (_name, call, route, body) => {
		const calls = stubFetch(() => jsonBody({ success: true }));
		await expect(call()).resolves.toEqual({ success: true });
		expect(calls).toHaveLength(1);
		const [method, at] = route.split(' ');
		expect(`${calls[0].init?.method} ${calls[0].url}`).toBe(`${method} /api${at}`);
		// no body for a command without one
		expect(calls[0].init?.body === undefined ? undefined : sentBody(calls[0])).toEqual(body);
	});

	it('sends an IR binding as the body itself', async () => {
		const calls = stubFetch(() => jsonBody({ success: true }));
		const binding = { name: 'Volume +', actions: [] } as unknown as Parameters<typeof irApi.setBinding>[1];
		await irApi.setBinding(7, binding);
		expect(calls[0].url).toBe('/api/ir/keymap/7');
		expect(calls[0].init?.method).toBe('PUT');
		expect(sentBody(calls[0])).toEqual(binding);
	});

	it('sends TV keys and config as given', async () => {
		const calls = stubFetch(() => jsonBody({ success: true }));
		await tvApi.sendKey('VolumeUp' as Parameters<typeof tvApi.sendKey>[0]);
		await tvApi.setConfig({ host: 'tv' } as Parameters<typeof tvApi.setConfig>[0]);
		expect(sentBody(calls[0])).toEqual({ key: 'VolumeUp' });
		expect(`${calls[1].init?.method} ${calls[1].url}`).toBe('PUT /api/tv/config');
		expect(sentBody(calls[1])).toEqual({ host: 'tv' });
	});
});

/** A stand-in XMLHttpRequest: the test drives the upload and the answer. */
class FakeXhr {
	static last: FakeXhr;
	method = '';
	url = '';
	withCredentials = false;
	responseType = '';
	status = 0;
	response: unknown = null;
	body: unknown;
	upload: { onprogress?: (e: { lengthComputable: boolean; loaded: number; total: number }) => void; onload?: () => void } = {};
	onerror?: () => void;
	ontimeout?: () => void;
	onload?: () => void;
	timeout = 0;
	constructor() {
		FakeXhr.last = this;
	}
	open(method: string, url: string) {
		this.method = method;
		this.url = url;
	}
	send(body: unknown) {
		this.body = body;
	}
	answer(status: number, response: unknown) {
		this.status = status;
		this.response = response;
		this.onload?.();
	}
}

describe('androidTvApi.installApk', () => {
	afterEach(() => vi.unstubAllGlobals());
	const apk = () => new File(['apk'], 'app.apk');

	it('uploads the file as multipart with the session cookie, and reports progress', async () => {
		vi.stubGlobal('XMLHttpRequest', FakeXhr);
		const progress = vi.fn();
		const done = androidTvApi.installApk(apk(), progress);
		const xhr = FakeXhr.last;
		expect(`${xhr.method} ${xhr.url}`).toBe('POST /api/androidtv/apk');
		expect(xhr.withCredentials).toBe(true);
		expect((xhr.body as FormData).get('apk')).toBeInstanceOf(File);
		xhr.upload.onprogress?.({ lengthComputable: true, loaded: 25, total: 100 });
		xhr.upload.onprogress?.({ lengthComputable: false, loaded: 50, total: 0 });
		xhr.upload.onload?.();
		expect(progress.mock.calls).toEqual([[0.25], [1]]);
		xhr.answer(200, { success: true, message: 'installed' });
		await expect(done).resolves.toEqual({ success: true, message: 'installed' });
	});

	it('fails with the server’s words and its refusal’s name, or its status', async () => {
		vi.stubGlobal('XMLHttpRequest', FakeXhr);
		const refused = androidTvApi.installApk(apk());
		FakeXhr.last.answer(500, { error: 'INSTALL_FAILED_VERSION_DOWNGRADE' });
		await expect(refused).rejects.toThrow('INSTALL_FAILED_VERSION_DOWNGRADE');
		const forbidden = androidTvApi.installApk(apk());
		FakeXhr.last.answer(403, { success: false, error: 'Admins only', code: 'forbidden' });
		await expect(forbidden).rejects.toMatchObject({ status: 403, code: 'forbidden' });
		const silent = androidTvApi.installApk(apk());
		FakeXhr.last.answer(413, null);
		await expect(silent).rejects.toMatchObject({ status: 413 });
	});

	it('fails as « unreachable » when the network drops, or after its time limit', async () => {
		vi.stubGlobal('XMLHttpRequest', FakeXhr);
		const lost = androidTvApi.installApk(apk());
		expect(FakeXhr.last.timeout).toBeGreaterThan(0);
		FakeXhr.last.onerror?.();
		await expect(lost).rejects.toMatchObject({ code: UNREACHABLE });
		const slow = androidTvApi.installApk(apk());
		FakeXhr.last.ontimeout?.();
		await expect(slow).rejects.toBeInstanceOf(ApiError);
	});

	it('renews an expired session once, then uploads again', async () => {
		vi.stubGlobal('XMLHttpRequest', FakeXhr);
		const calls = stubFetch(() => jsonBody({}));
		const done = androidTvApi.installApk(apk());
		const first = FakeXhr.last;
		first.answer(401, { error: 'expired' });
		await vi.waitFor(() => expect(FakeXhr.last).not.toBe(first));
		expect(calls.map((c) => c.url)).toEqual(['/api/auth/refresh']);
		FakeXhr.last.answer(200, { success: true, message: 'installed' });
		await expect(done).resolves.toEqual({ success: true, message: 'installed' });
	});
});
