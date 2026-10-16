// The wall frame's native app (frame/ui) against the real backend, as the frame runs it: the
// simulated house (house.ts, the same one the pages see) answers the device routes through a
// small proxy, everything else goes to the backend. The app pairs as a wall screen (its code
// given from an admin's phone), shows the house, and a finger acts on it: a lamp toggled, its
// brightness slid, a plug turned off, a shutter closed, the column scrolled. Its own window is
// nowhere (SDL's dummy driver, software rendering). `FRAME_SIM=armv7` runs the frame's own
// binary instead, on the frame's system emulated, under cage, its GLES 2 from Mesa
// (frame/ui/simulate.sh): slower, the waits are longer. Pictures land in SHOTS.
//
// Needs: `cargo build --manifest-path frame/ui/Cargo.toml` (or `frame/ui/build.sh` for armv7).
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { LAMP_LIMIT_MS } from '../web/src/lib/paces.ts';
import { PAIRING_CODE, SHOTS, BASE, check, done, fr, launch, open, pairScreen, removeScreen, signIn, until } from './lib.ts';
import { house } from './house.ts';

const ARMV7 = process.env.FRAME_SIM === 'armv7';
const APP = new URL(ARMV7 ? '../frame/ui/simulate.sh' : '../frame/ui/target/debug/maison-frame-ui', import.meta.url).pathname;
/** Emulated, every wait is this many times longer. */
const SLOW = ARMV7 ? 4 : 1;
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
	return until(() => readSince(path, now), timeout * SLOW);
};

/** How many event streams the backend opened for the app (a 200 on GET /api/events). */
let streamsOpened = 0;

// the proxy: the house answers its routes, the backend the rest (its cookies passed both ways)
const proxy = createServer(async (req, res) => {
	const chunks: Buffer[] = [];
	for await (const c of req) chunks.push(c as Buffer);
	const text = Buffer.concat(chunks).toString();
	const url = new URL(req.url ?? '/', 'http://proxy');
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
	if (url.pathname === '/api/events' && back.status === 200) streamsOpened++;
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
		// from the container, the Mac is host.docker.internal
		MAISON_TARGET: `http://${ARMV7 ? 'host.docker.internal' : 'localhost'}:${port}`,
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
	await until(() => said.slice(from).some(answered) || gone, 10_000 * SLOW);
	const line = said.slice(from).find(answered);
	if (!line || line.startsWith('error')) throw new Error(`${command}: ${line ?? 'no answer'}`);
}
/** Tells `command` once its control is drawn: a read answered is not yet a drawing (emulated, the
 * app takes its time), so a « no control » or « no slider » is asked again, within tell's wait. */
async function shown(command: string) {
	const deadline = Date.now() + 10_000 * SLOW;
	for (;;) {
		try {
			return await tell(command);
		} catch (e) {
			if (Date.now() > deadline || !/: no (control|slider) /.test(String(e))) throw e;
			await new Promise((r) => setTimeout(r, 200));
		}
	}
}
async function saidLine(pred: (l: string) => boolean, timeout = 10_000) {
	await until(() => said.some(pred) || gone, timeout * SLOW);
	return said.some(pred);
}
let shots = 0;
async function picture(name: string) {
	if (!SHOTS) return;
	const path = `${process.cwd()}/${SHOTS}/frame${ARMV7 ? '-armv7' : ''}-${++shots}-${name}.png`;
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
check('paired, it follows the server’s changes at once', await until(() => streamsOpened === 1, 3_000 * SLOW), `${streamsOpened} opened`);
check(
	'it reads the lamps, the plugs, the shutters and Tempo',
	await until(() => ['/api/hue-lamps', '/api/meross', '/api/matter/covers'].every((p) => state.asked.includes(p)), 10_000 * SLOW),
	state.asked.join(' ')
);
// the plug's power answered, and the app on to its next read: what it read is on screen
await until(() => readSince('/api/meross/plug-1/electricity', 0), 10_000 * SLOW);
const after = reads.length;
await until(() => reads.slice(after).some((r) => r.answered), 10_000 * SLOW);
await picture('house');

// Tempo's page, from its tile, and back
await tell('press open:tempo');
check('the Tempo tile opens its page', await saidLine((l) => l === `screen page ${fr.nav_tempo()}`, 10_000));
await picture('tempo');
await tell('press open:house');
check('« Retour à l’accueil » brings the house back', await saidLine((l) => l === 'screen house', 10_000));

// a finger: the lamp off, its brightness, the plug, the shutter
const sent = (want: string) => until(() => state.sent.some((s) => s.startsWith(want)), 5_000 * SLOW);
await tell('press hue-lamp:hue-01:power');
check('a tap on the lamp’s icon turns it off', await sent('POST /api/hue-lamps/hue-01/power {"enabled":false}'), state.sent.join(' | '));
await tell('press hue-lamp:hue-01:power');
check('and on again', await sent('POST /api/hue-lamps/hue-01/power {"enabled":true}'));
await until(() => state.hue[0].state.isOn, 5_000 * SLOW);
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
// past the lamps' limit the app gives up on its own: its next read of them after the limit has
// it drawn « Pas de réponse » (the same limit as the web app's, paces.ts)
const pressed = Date.now();
await until(() => readSince('/api/hue-lamps', pressed + LAMP_LIMIT_MS * SLOW), 15_000 * SLOW);
await picture('no-answer');
state.silent.delete('hue');

// the server's changes: one stream, opened once paired and kept open all along (never a loop of
// reconnections)
check('the app follows the event stream, kept open', streamsOpened === 1, `${streamsOpened} opened`);

// removed in « Mon compte »: the app says so, then offers a new code
await removeScreen(phone, 'Cadre natif');
check('removed, the app says so', await saidLine((l) => l === `screen door ${fr.screen_revoked_title()}`, 15_000));
await picture('revoked');
check('the app is still running', !gone);

end();
await done(browser);
