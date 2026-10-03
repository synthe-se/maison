// axe on each screen, light and dark, desktop and phone (WCAG 2.2 AA tags) — docs/ux/
// tableau-de-bord.md « Ce que Maison applique » 15.
import type { Page } from 'playwright';
import { BASE, PHONE, axe, done, launch, open, signIn } from './lib.ts';
import { house } from './house.ts';

let failures = 0;

async function audit(p: Page, label: string) {
	const r = (await axe(p, ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'])).map(
		(v) => `${v.impact} ${v.id}: ${v.help} (${v.nodes.length}) e.g. ${v.nodes[0]?.target}`
	);
	failures += r.length;
	console.log(`${label}: ${r.length ? '\n  ' + r.join('\n  ') : 'no violations'}`);
}

const SCREENS: [string, string][] = [
	['/', 'home'],
	['/remote', 'remote'],
	['/tempo-predictions', 'tempo'],
	['/device/feeder-1', 'feeder'],
	['/hue-lamp/hue-01', 'lamp'],
	['/meross/plug-1', 'plug']
];

const browser = await launch();
for (const scheme of ['light', 'dark'] as const) {
	for (const viewport of [{ width: 1280, height: 900 }, PHONE]) {
		// axe is injected as an inline script: the app's CSP would (rightly) refuse it
		const p = await open(browser, { colorScheme: scheme, viewport, bypassCSP: true });
		const where = `${scheme} ${viewport.width}px`;
		await house().serve(p);
		await p.goto(BASE + '/');
		await p.getByRole('button', { name: 'Se connecter' }).waitFor();
		await audit(p, `${where} sign-in`);
		await signIn(p);
		for (const [path, name] of SCREENS) {
			await p.goto(BASE + path);
			await p.locator('main h1').first().waitFor();
			await p.waitForTimeout(600);
			await audit(p, `${where} ${name}`);
		}
		// the session panel, opened
		await p.goto(BASE + '/');
		await p.locator('main h1').first().waitFor();
		await p.getByRole('button', { name: /Session de/ }).click();
		await audit(p, `${where} session panel`);
		await p.keyboard.press('Escape');
		// a confirmation dialog (removing a shutter)
		await p.getByRole('article', { name: 'Volet salon', exact: true }).getByRole('button', { name: 'Réglages de Volet salon' }).click();
		await p.getByRole('button', { name: 'Retirer Volet salon' }).click();
		await audit(p, `${where} confirmation dialog`);
		await p.keyboard.press('Escape');
		await p.context().close();
	}
}
await done(browser, failures === 0);
