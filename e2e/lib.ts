// What every scenario repeats, said once (after Ariane's e2e/lib.ts): the server, checks and
// the exit code, a signed-in page, errors watched, screenshots, axe.
import { chromium, type Browser, type BrowserContextOptions, type Locator, type Page } from 'playwright';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { m } from '../web/src/lib/paraglide/messages.js';

/** The backend under test; run.sh passes it. */
export const BASE = process.env.BASE ?? 'http://localhost:3099';
/** Where screenshots go; none taken when unset. */
export const SHOTS = process.env.SHOTS;
export const PHONE = { width: 390, height: 844 };
/** The account every scenario signs in as (an admin, made by invitation). */
export const USER = { id: 'e2e', name: 'E2E' };

// ---- the app's words

/**
 * The French UI's words, as the app says them: i18n/messages/fr.json compiled by Paraglide
 * (web/src/lib/paraglide), so a scenario never hard-codes a text the app may reword. The
 * scenarios run in French (`open`).
 */
export const fr: typeof m = new Proxy(m, {
	get: (messages, key) => {
		const say = Reflect.get(messages, key) as (inputs: object, options: { locale: 'fr' }) => string;
		return (inputs: object = {}) => say(inputs, { locale: 'fr' });
	}
});

// ---- checks and the exit code

let ok = true;
let quiet = false;

/** Prints `label: true|false` (and the detail, if any); any false fails the run. */
export function check(label: string, cond: boolean, detail = '') {
	if (!cond || !quiet) console.log(`${label}: ${cond}${detail ? ` ${detail}` : ''}`);
	ok &&= cond;
}

/** From now on, only the failed checks are printed (long measured grids). */
export function onlyFailures() {
	quiet = true;
}

/** Polls `cond` until it holds (true) or `timeout` ms pass (false): a wait on what the page or
 * the simulated house says, never on a fixed delay. */
export async function until(cond: () => boolean | Promise<boolean>, timeout = 5_000): Promise<boolean> {
	const end = Date.now() + timeout;
	for (;;) {
		if (await cond()) return true;
		if (Date.now() > end) return false;
		await new Promise((r) => setTimeout(r, 25));
	}
}

/** Waits for a page to be still: its fonts loaded, no skeleton nor busy list left, two frames
 * painted (what an axe audit, a measure or a screenshot needs). */
export async function settled(page: Page, timeout = 15_000) {
	await page.waitForFunction(() => !document.querySelector('.skeleton, [aria-busy="true"]'), undefined, { timeout }).catch(() => {});
	await page.evaluate(async () => {
		await document.fonts.ready;
		await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
	});
}

/** Closes the browser and exits: 0 when every check (and `also`) held. */
export async function done(browser?: Browser, also = true): Promise<never> {
	await browser?.close();
	process.exit(ok && also ? 0 : 1);
}

// ---- browsers, pages, errors

export const launch = () => chromium.launch();

/** A page in a fresh context (its own storage), French unless said otherwise. */
export async function open(browser: Browser, options: BrowserContextOptions = {}): Promise<Page> {
	const page = await (await browser.newContext({ locale: 'fr-FR', ...options })).newPage();
	watchErrors(page);
	return page;
}

const errors: string[] = [];
/** Expected, not errors: a device absent from the throwaway root answers 404/500/503, a
 * session not yet opened answers 401, a refusal the app words itself 403/409 (the last
 * passkey, a member asking for invitations). */
const EXPECTED = /status of (401|403|404|409|500|502|503)/;

function watchErrors(page: Page) {
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (msg) => msg.type() === 'error' && !EXPECTED.test(msg.text()) && errors.push(msg.text()));
}

/** The check every scenario ends with; the errors themselves printed when there are some. */
export function checkNoErrors(label = 'no page errors') {
	check(label, errors.length === 0);
	if (errors.length) console.log(errors);
}

// ---- signing in

/** Waits for the page's own title (h1). */
export const title = (page: Page, name: string | RegExp) => page.getByRole('heading', { name, level: 1 }).waitFor();

/** A one-time invitation link, made as on the Pi: `maison-backend invite` (run.sh says where). */
export function invitation(person = USER.id, name = USER.name, admin = true): string {
	const args = ['invite', person, '--name', name, ...(admin ? ['--admin'] : [])];
	const out = execFileSync(process.env.MAISON_BIN!, args, { encoding: 'utf8' });
	return out.match(/http\S+\/invite\/\S+/)![0];
}

/** A platform authenticator with user verification, as on a phone or a Mac (WebAuthn CDP). */
export async function authenticator(page: Page) {
	const cdp = await page.context().newCDPSession(page);
	await cdp.send('WebAuthn.enable');
	const add = () =>
		cdp.send('WebAuthn.addVirtualAuthenticator', {
			options: {
				protocol: 'ctap2',
				transport: 'internal',
				hasResidentKey: true,
				hasUserVerification: true,
				isUserVerified: true,
				automaticPresenceSimulation: true
			}
		});
	return { cdp, add, ...(await add()) };
}

/** In by a fresh invitation (a new passkey on a new authenticator), then the page itself. */
export async function signIn(page: Page, path = '/') {
	await authenticator(page);
	await page.goto(invitation());
	await page.getByRole('button', { name: fr.invite_create() }).click();
	await page.getByRole('button', { name: fr.invite_enter() }).click();
	await page.waitForURL((url) => !url.pathname.startsWith('/invite/'));
	if (path !== '/') await page.goto(BASE + path);
	await page.locator('main h1').first().waitFor();
}

/** Scrolls sideways: wider than the window. */
export const sideways = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth > innerWidth);

/** Waits for a toast saying `text`. */
export async function toast(page: Page, text: string | RegExp) {
	const t = page.locator('.toast', { hasText: text });
	await t.waitFor();
	return t;
}

// ---- looking

/** A screenshot into SHOTS (nothing when unset): a page, whole if `full`, or one element. */
export async function shot(target: Page | Locator, name: string, full = false) {
	if (!SHOTS) return;
	const path = `${SHOTS}/${name}.png`;
	await ('goto' in target ? target.screenshot({ path, fullPage: full }) : target.screenshot({ path }));
}

let axeSource: string | undefined;
export type Violation = { id: string; impact: string; help: string; nodes: { target: unknown }[] };

/** axe-core on the page as it is, restricted to `tags`. */
export async function axe(page: Page, tags: string[]): Promise<Violation[]> {
	axeSource ??= readFileSync(new URL('node_modules/axe-core/axe.min.js', import.meta.url), 'utf8');
	await page.addScriptTag({ content: axeSource });
	// @ts-expect-error injected
	return page.evaluate(async (runOnly) => (await window.axe.run(document, { runOnly })).violations, tags);
}
