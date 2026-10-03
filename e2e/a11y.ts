// axe on each screen, light and dark, desktop and phone (WCAG 2.2 AA tags) — docs/ux.md
// « Checks ».
import type { Page } from 'playwright';
import { BASE, PHONE, axe, done, fr, invitation, launch, open, settled, signIn } from './lib.ts';
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
	['/meross/plug-1', 'plug'],
	['/tv', 'tv'],
	['/androidtv', 'box']
];

const browser = await launch();
for (const scheme of ['light', 'dark'] as const) {
	for (const viewport of [{ width: 1280, height: 900 }, PHONE]) {
		// axe is injected as an inline script: the app's CSP would (rightly) refuse it
		const p = await open(browser, { colorScheme: scheme, viewport, bypassCSP: true });
		const where = `${scheme} ${viewport.width}px`;
		await house().serve(p);
		await p.goto(BASE + '/');
		await p.getByRole('button', { name: fr.pk_button() }).waitFor();
		await audit(p, `${where} sign-in`);
		// the invitation, in the same door (a link made for the audit, never used)
		await p.goto(invitation('door', 'Porte', false));
		await p.getByRole('heading', { level: 1, name: fr.invite_title({ name: 'Porte' }) }).waitFor();
		await audit(p, `${where} invitation`);
		await signIn(p);
		for (const [path, name] of SCREENS) {
			await p.goto(BASE + path);
			await p.locator('main h1').first().waitFor();
			await settled(p);
			await audit(p, `${where} ${name}`);
		}
		// the session panel, opened
		await p.goto(BASE + '/');
		await p.locator('main h1').first().waitFor();
		await p.getByRole('button', { name: fr.session_menu({ name: 'E2E' }) }).click();
		await audit(p, `${where} session panel`);
		await p.keyboard.press('Escape');
		// a confirmation dialog (removing a shutter)
		await p
			.getByRole('article', { name: 'Volet salon', exact: true })
			.getByRole('button', { name: fr.common_settings_of({ name: 'Volet salon' }) })
			.click();
		await p.getByRole('button', { name: fr.common_remove_named({ name: 'Volet salon' }) }).click();
		await audit(p, `${where} confirmation dialog`);
		await p.keyboard.press('Escape');
		await p.context().close();
	}
}
await done(browser, failures === 0);
