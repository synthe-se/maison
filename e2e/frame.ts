// The wall frame's native app (frame/ui) against the real backend, as the frame runs it: the
// simulated house (house.ts, the same one the pages see) answers the device routes through a
// small proxy, everything else goes to the backend. The app pairs as a wall screen (its code
// given from an admin's phone), shows the house, and a finger acts on it: a lamp toggled, its
// brightness slid, a plug turned off, a shutter closed, the column scrolled, every device's page,
// the remotes' keys, a reset asked first, Maison gone and back, the family's photos taking over
// when nobody is there. Its own window is nowhere (SDL's
// dummy driver, software rendering). Pictures land in SHOTS.
//
// Needs: `cargo build --manifest-path frame/ui/Cargo.toml`.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { existsSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { LAMP_LIMIT_MS } from '../web/src/lib/paces.ts';
import type { BroadlinkClimateStateResponse } from '../web/src/lib/devices/climate/api.ts';
import type { AndroidTvStatusResponse, TvStatusResponse } from '../web/src/lib/devices/tv/api.ts';
import {
	PAIRING_CODE,
	SHOTS,
	BASE,
	check,
	done,
	fr,
	launch,
	open,
	pairScreen,
	photoFile,
	removeScreen,
	signIn,
	toast,
	until,
	visit
} from './lib.ts';
import { house } from './house.ts';

const APP = new URL('../frame/ui/target/debug/maison-frame-ui', import.meta.url).pathname;
/** Emulated, every wait is this many times longer. */
const { state, answer } = house();

/** Every read the house answered for the app, when it was asked and once it was answered: what
 * the waits below wait on (never a fixed delay). */
const reads: { path: string; at: number; answered: boolean }[] = [];
/** The app has read `path` (an answer sent) since `since` (ms), `times` times. */
const readSince = (path: string, since: number, times = 1) =>
	reads.filter((r) => r.path === path && r.at >= since && r.answered).length >= times;
/** Waits for the app to read `path` again: answered once more after now. */
const readAgain = (path: string, timeout = 15_000) => {
	const now = Date.now();
	return until(() => readSince(path, now), timeout);
};

/** How many event streams the backend opened for the app (a 200 on GET /api/events). */
let streamsOpened = 0;
/** Maison gone: nothing answers, the open stream drops. */
let down = false;
let stream: { destroy: () => void } | undefined;

state.hue[0].state.temperature = 40;
/** What only the wall's pages ask of the house: the remotes, the air conditioner's last order. */
const tv: TvStatusResponse = {
	success: true,
	config: { host: '192.0.2.30' },
	status: {
		configured: true,
		power: 'on',
		name: 'Philips',
		volume: { current: 12, min: 0, max: 60, muted: false },
		ambilight: { power: true, mode: 'lounge', style: 'STANDARD' }
	}
};
const box: AndroidTvStatusResponse = {
	success: true,
	config: { host: '192.0.2.40' },
	status: { configured: true, reachable: true, awake: true, model: 'LEAP-S1', paired: true }
};
const climate: BroadlinkClimateStateResponse = {
	success: true,
	state: {
		power: true,
		lastCommand: 'state-cool-21-fan-auto-vane-auto',
		lastOnCommand: 'state-cool-21-fan-auto-vane-auto',
		settings: { mode: 'cool', temperature: 21, fan: 'auto', vane: 'auto', econo: false, stopInMinutes: null },
		host: '192.0.2.60',
		model: 'msz-hj5va',
		updatedAt: new Date().toISOString()
	},
	message: ''
};
function remotes(method: string, pathname: string, text: string): string | undefined {
	const orders = /^\/api\/(tv\/(key|volume|ambilight)|androidtv\/key|broadlink\/mitsubishi\/send|hue-lamps\/[^/]+\/temperature)/;
	if (method !== 'GET' && orders.test(pathname)) {
		state.sent.push(`${method} ${pathname} ${text}`);
		return JSON.stringify({ success: true, message: 'ok' });
	}
	if (method !== 'GET') return undefined;
	if (pathname === '/api/tv') return JSON.stringify(tv);
	if (pathname === '/api/androidtv') return JSON.stringify(box);
	if (pathname === '/api/broadlink/mitsubishi/state') return JSON.stringify(climate);
	return undefined;
}

// the proxy: the house answers its routes, the backend the rest (its cookies passed both ways)
const proxy = createServer(async (req, res) => {
	const chunks: Buffer[] = [];
	for await (const c of req) chunks.push(c as Buffer);
	const text = Buffer.concat(chunks).toString();
	const url = new URL(req.url ?? '/', 'http://proxy');
	if (down) return void req.socket.destroy();
	const own = remotes(req.method ?? 'GET', url.pathname, text);
	if (own) {
		res.writeHead(200, { 'content-type': 'application/json' }).end(own);
		return;
	}
	// a screen's own routes go straight to the backend, never into the house's log (its
	// pairing secret is no test's to print)
	const device = url.pathname.startsWith('/api/') && !url.pathname.startsWith('/api/screens/');
	const read = device && req.method === 'GET' ? { path: url.pathname, at: Date.now(), answered: false } : undefined;
	if (read) reads.push(read);
	const a = device ? await answer(req.method ?? 'GET', url.pathname, text ? JSON.parse(text) : null) : undefined;
	if (a === 'silent') return; // never answers
	if (a) {
		res.writeHead(200, { 'content-type': 'application/json' }).end(a.body, () => read && (read.answered = true));
		return;
	}
	const headers = new Headers();
	for (const [k, v] of Object.entries(req.headers)) if (typeof v === 'string') headers.set(k, v);
	// the app gone (an event stream it closed): the backend's answer is let go too
	const left = new AbortController();
	res.on('close', () => left.abort());
	const back = await fetch(BASE + url.pathname + url.search, {
		method: req.method,
		headers,
		body: text || undefined,
		redirect: 'manual',
		signal: left.signal
	});
	const out: Record<string, string | string[]> = {};
	back.headers.forEach((v, k) => {
		if (k !== 'content-encoding' && k !== 'content-length' && k !== 'set-cookie') out[k] = v;
	});
	const cookies = back.headers.getSetCookie();
	if (cookies.length) out['set-cookie'] = cookies;
	if (url.pathname === '/api/events' && back.status === 200) {
		streamsOpened++;
		stream = res;
	}
	// passed on as it comes: the event stream is an answer that never ends
	res.writeHead(back.status, out);
	try {
		if (back.body) for await (const chunk of back.body) res.write(chunk);
	} catch {
		// the app closed it
	}
	res.end();
});
await new Promise<void>((r) => proxy.listen(0, r));
const port = (proxy.address() as AddressInfo).port;

// the app, its cookie in a throwaway directory, driven over stdin (sim.rs)
const stateDir = mkdtempSync(join(tmpdir(), 'maison-frame-'));
const app = spawn(APP, [], {
	env: {
		...process.env,
		MAISON_TARGET: `http://localhost:${port}`,
		MAISON_STATE: stateDir,
		MAISON_LANG: 'fr',
		MAISON_WINDOW: '800x1280',
		MAISON_STDIN: '1',
		SDL_VIDEO_DRIVER: 'dummy',
		SDL_RENDER_DRIVER: 'software'
	},
	stdio: ['pipe', 'pipe', 'inherit']
});
const said: string[] = [];
createInterface({ input: app.stdout! }).on('line', (l) => said.push(l));
let gone = false;
app.on('exit', (code) => {
	gone = true;
	if (code) console.error(`the app ended: ${code}`);
});
const end = () => {
	app.kill();
	proxy.close();
	rmSync(stateDir, { recursive: true, force: true });
};
process.on('exit', end);

/** Sends a command; waits for the line that answers it. */
async function tell(command: string, answered = (l: string) => l === `ok ${command}` || l.startsWith(`error ${command}`)) {
	const from = said.length;
	app.stdin!.write(command + '\n');
	await until(() => said.slice(from).some(answered) || gone, 10_000);
	const line = said.slice(from).find(answered);
	if (!line || line.startsWith('error')) throw new Error(`${command}: ${line ?? 'no answer'}`);
}
/** Tells `command` once its control is drawn: a read answered is not yet a drawing (emulated, the
 * app takes its time), so a « no control » or « no slider » is asked again, within tell's wait. */
async function shown(command: string) {
	const deadline = Date.now() + 10_000;
	for (;;) {
		try {
			return await tell(command);
		} catch (e) {
			if (Date.now() > deadline || !/: no (control|slider) /.test(String(e))) throw e;
			await new Promise((r) => setTimeout(r, 200));
		}
	}
}
/** Tells `command` on a control further down the column: scrolled to until it is drawn. */
async function reach(command: string) {
	for (let i = 0; ; i++) {
		try {
			return await tell(command);
		} catch (e) {
			if (i > 8 || !/: no control /.test(String(e))) throw e;
			await tell('drag 1100 500');
		}
	}
}
async function saidLine(pred: (l: string) => boolean, timeout = 10_000) {
	await until(() => said.some(pred) || gone, timeout);
	return said.some(pred);
}
let shots = 0;
async function picture(name: string) {
	if (!SHOTS) return;
	const path = `${process.cwd()}/${SHOTS}/frame-${++shots}-${name}.png`;
	await tell(`shot ${path}`, (l) => l === `shot ${path}`);
}

// signed out: a code to give
check('the app shows the pairing door', await saidLine((l) => l === `screen door ${fr.screen_pair_title()}`));
check('with a code', await saidLine((l) => l.startsWith('code ')));
const code = said.find((l) => l.startsWith('code '))!.slice(5);
check('the code is the backend’s kind', PAIRING_CODE.test(code), code);
await picture('pairing');

// the admin pairs it from their phone, in « Mon compte »
const browser = await launch();
const phone = await open(browser);
await signIn(phone, '/account');
await pairScreen(phone, code, 'Cadre natif');

// it opens by itself, and reads the house
check('paired, the app shows the house', await saidLine((l) => l === 'screen house', 15_000));
check('paired, it follows the server’s changes at once', await until(() => streamsOpened === 1, 3_000), `${streamsOpened} opened`);
check(
	'it reads the lamps, the plugs, the shutters and Tempo',
	await until(() => ['/api/hue-lamps', '/api/meross', '/api/matter/covers'].every((p) => state.asked.includes(p)), 10_000),
	state.asked.join(' ')
);
// the plug's power answered, and the app on to its next read: what it read is on screen
await until(() => readSince('/api/meross/plug-1/electricity', 0), 10_000);
const after = reads.length;
await until(() => reads.slice(after).some((r) => r.answered), 10_000);
await picture('house');

// Tempo's page, from its tile, and back
await tell('press open:tempo');
check('the Tempo tile opens its page', await saidLine((l) => l === `screen page ${fr.nav_tempo()}`, 10_000));
await picture('tempo');
await tell('press open:house');
check('« Retour à l’accueil » brings the house back', await saidLine((l) => l === 'screen house', 10_000));

// the litter box's page, from its row: offline here, its connection switched on from the wall
await tell('press open:device:litter-1');
check('a cat device’s row opens its page', await saidLine((l) => l === 'screen page Litière', 10_000));
await picture('litter-box');
await tell('press tuya:litter-1:connection');
check(
	'its connection switch reconnects it',
	await until(() => state.sent.some((s) => s.startsWith('POST /api/devices/litter-1/connect')), 5_000),
	state.sent.join(' | ')
);
const houses = said.filter((l) => l === 'screen house').length;
await tell('press open:house');
check('and back to the house', await until(() => said.filter((l) => l === 'screen house').length > houses, 10_000));

// a finger: the lamp off, its brightness, the plug, the shutter
const sent = (want: string) => until(() => state.sent.some((s) => s.startsWith(want)), 5_000);
await tell('press hue-lamp:hue-01:power');
check('a tap on the lamp’s icon turns it off', await sent('POST /api/hue-lamps/hue-01/power {"enabled":false}'), state.sent.join(' | '));
await tell('press hue-lamp:hue-01:power');
check('and on again', await sent('POST /api/hue-lamps/hue-01/power {"enabled":true}'));
await until(() => state.hue[0].state.isOn, 5_000);
// the next read of the lamps shows it on, with its slider
check('the app reads the lamp on again', await readAgain('/api/hue-lamps'));
await shown('slide hue-lamp:hue-01:brightness 0.25');
check('the brightness slider sends where it is let go', await sent('POST /api/hue-lamps/hue-01/brightness'), state.sent.join(' | '));
await tell('press broadlink:climate:send', (l) => l.startsWith('ok') || l.startsWith('error'));
check(
	'an air conditioner button sends its order through the blaster found (on and off share one key: one order at a time)',
	await sent('POST /api/broadlink/mitsubishi/send'),
	state.sent.join(' | ')
);
await tell('press meross:plug-1:toggle');
check('the plug turns off', await sent('POST /api/meross/plug-1/toggle {"on":false}'));
await tell('press shutters:0000000000000002:move', (l) => l.startsWith('ok') || l.startsWith('error'));
check('a shutter button moves it', await sent('POST /api/matter/covers/0000000000000002/'), state.sent.join(' | '));
await picture('after-gestures');
await tell('drag 1100 300');
await picture('scrolled');

// a lamp that never answers: « Pas de réponse · Réessayer » past its 3 s
state.silent.add('hue');
await tell('drag 300 1100');
await tell('press hue-lamp:hue-01:power');
// past the lamps' limit the app gives up on its own and draws « Pas de réponse » (the same limit
// as the web app's, paces.ts)
const pressed = Date.now();
await until(() => readSince('/api/hue-lamps', pressed + LAMP_LIMIT_MS), 15_000);
await picture('no-answer');
state.silent.delete('hue');

// the server's changes: one stream, opened once paired and kept open all along (never a loop of
// reconnections)
check('the app follows the event stream, kept open', streamsOpened === 1, `${streamsOpened} opened`);

// « Maintenant »: a chip leads to its group
await tell('press open:tempo');
await tell('press open:house');
await shown('press jump:lamps');
await picture('jumped');
await tell('press open:tempo');
await tell('press open:house');

// each device's page, from its row: what a person at home does there reaches the device
const page = async (door: string, title: string) => {
	await reach(`press ${door}`);
	check(`${door} opens its page`, await saidLine((l) => l === `screen page ${title}`, 10_000));
	await tell('keys');
};
await page('open:tv', 'Philips');
await picture('tv');
await reach('press tv:key:cursor_up');
check('a pad key reaches the TV', await sent('POST /api/tv/key {"key":"cursor_up"}'), state.sent.join(' | '));
await reach('press tv:key:play_pause');
check('and a media key', await sent('POST /api/tv/key {"key":"play_pause"}'));
await reach('press tv:ambilight');
check('Ambilight too', await sent('POST /api/tv/ambilight'));
await tell('press open:house');
await page('open:box', 'LEAP-S1');
await picture('box');
await reach('press androidtv:key:ok');
check('a key reaches the box', await sent('POST /api/androidtv/key {"key":"ok"}'), state.sent.join(' | '));
await reach('press androidtv:key:volume_up');
check('its volume too', await sent('POST /api/androidtv/key {"key":"volume_up"}'));
await tell('press open:house');
await page('open:climate', fr.climate_brand_name());
await picture('climate');
await tell('press open:house');
await page('open:hue-lamp:hue-01', 'Salon');
await picture('lamp');
await shown('slide hue-lamp:hue-01:temperature 0.75');
check('a lamp’s white follows its slider', await sent('POST /api/hue-lamps/hue-01/temperature'), state.sent.join(' | '));
await tell('press open:house');
await page('open:plug:plug-1', 'Lave-linge');
await readAgain('/api/meross/plug-1/consumption');
await tell('keys');
await picture('plug');
await tell('press open:house');
await page('open:device:fountain-1', 'Fontaine');
await reach('press ask:tuya:fountain-1:water');
await picture('asked');
check('a reset waits for its answer', !state.sent.some((s) => s.includes('/fountain/reset/water')));
await tell('press dialog:keep');
check('« Garder » keeps it', !state.sent.some((s) => s.includes('/fountain/reset/water')));
await reach('press ask:tuya:fountain-1:water');
await tell('press dialog:sure');
check('the verb sends it', await sent('POST /api/devices/fountain-1/fountain/reset/water'), state.sent.join(' | '));
await tell('press open:house');
await until(() => said.at(-1) === 'screen house' || said.includes('screen house'), 5_000);
await picture('home-again');

// Maison gone: no gesture left, one band for the house; back by itself at the first answer
down = true;
stream?.destroy();
const noGesture = async () => {
	const from = said.length;
	app.stdin!.write('press meross:plug-1:toggle\n');
	await until(() => said.slice(from).some((l) => l.includes('press meross:plug-1:toggle')), 5_000);
	return said.slice(from).some((l) => l.startsWith('error') && l.includes('no control'));
};
await tell('drag 300 1100');
check('Maison gone, no gesture is reachable', await until(noGesture, 15_000));
await picture('gone');
down = false;
await tell('drag 300 1100');
check('it comes back by itself', await until(async () => !(await noGesture()), 40_000));

/** What only time can show: nothing happening, a fade running. */
const pause = (ms: number) => until(() => false, ms);

// the family's photos: added from the admin's phone, they reach the frame by themselves
const kept = () => (existsSync(join(stateDir, 'photos')) ? readdirSync(join(stateDir, 'photos')).filter((f) => f.endsWith('.jpg')) : []);
await visit(phone, '/photos');
await phone
	.getByLabel(fr.photos_add())
	.setInputFiles([await photoFile(phone, 'plage.png', '#e0a030'), await photoFile(phone, 'neige.png', '#3060c0')]);
await toast(phone, fr.photos_added({ count: 2 }));
check('added on the phone, they reach the frame by themselves', await until(() => kept().length === 2, 40_000), kept().join(' '));

// nobody at the frame: the house gives way to them
const isPhotos = (l: string) => l.startsWith('screen photos ');
const showing = () => said.findLast(isPhotos);
await tell('idle');
check('nobody there, it shows a photo', await saidLine(isPhotos));
const first = showing()!;
check('one of those kept', kept().includes(`${first.slice('screen photos '.length)}.jpg`), first);
await pause(1_200); // its fade over
await picture('photos');
const quiet = reads.length;
await pause(6_000);
check(
	'while they show, the house is not read',
	!reads.slice(quiet).some((r) => /\/(hue-lamps|meross|matter\/covers)/.test(r.path)),
	reads
		.slice(quiet)
		.map((r) => r.path)
		.join(' ')
);
await tell('turn');
check('its time up, the other one comes in', await until(() => showing() !== first, 10_000), showing());
await pause(300); // in the middle of its fade
await picture('photos-fade');

// Maison gone: they go on from the frame's own copy, and no band says anything
down = true;
stream?.destroy();
const before = showing();
await pause(1_500); // its fade over: a turn is for a photo at rest
await tell('turn');
check('Maison gone, they go on', await until(() => showing() !== before, 10_000), showing());
down = false;

// the first finger brings the house back, and does nothing else
const orders = state.sent.length;
const housesBefore = said.filter((l) => l === 'screen house').length;
await tell('tap 200 300');
check('a finger brings the house back', await until(() => said.filter((l) => l === 'screen house').length > housesBefore, 10_000));
check('and does nothing else', state.sent.length === orders, state.sent.slice(orders).join(' | '));

// removed on the phone, they leave the frame; with none left it only dims, as it did
for (const left of [2, 1]) {
	await phone.getByRole('button', { name: fr.common_remove_named({ name: fr.photos_alt({ number: 1 }) }) }).click();
	await phone.getByRole('alertdialog').getByRole('button', { name: fr.photos_remove_action() }).click();
	check(`${left} removed, the frame follows`, await until(() => kept().length === left - 1, 40_000), kept().join(' '));
	await (left > 1 ? phone.getByText(fr.photos_count({ count: 1 })) : phone.getByText(fr.photos_none(), { exact: true })).waitFor();
}
const afterRemoval = said.filter(isPhotos).length;
await tell('idle');
await pause(1_000);
check('no photo left: it stays on the house', said.filter(isPhotos).length === afterRemoval);
await tell('tap 200 300');

// removed in « Mon compte »: the app says so, then offers a new code; what it kept goes with it
await phone.getByLabel(fr.photos_add()).setInputFiles(await photoFile(phone, 'chat.png', '#40a060'));
await toast(phone, fr.photos_added({ count: 1 }));
check('a new photo reaches the frame', await until(() => kept().length === 1, 40_000));
await visit(phone, '/account');
await removeScreen(phone, 'Cadre natif');
// the event stream open, pushed sources read once a minute: a page opened reads at once
await tell('press open:tempo');
check('removed, the app says so', await saidLine((l) => l === `screen door ${fr.screen_revoked_title()}`, 15_000));
await picture('revoked');
check('a removed screen keeps no photo', await until(() => kept().length === 0, 10_000), kept().join(' '));
check('the app is still running', !gone);

end();
await done(browser);
