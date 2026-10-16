// axe on each screen, light and dark, desktop and phone (WCAG 2.2 AA tags) — docs/ux.md
// « Checks ».
import { BASE, PAIRING_CODE, PHONE, SCREENS, audit, done, fr, invitation, launch, open, signIn, visit } from './lib.ts';
import { house } from './house.ts';

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
		// a wall screen's pairing: its code shown, read out
		await p.getByRole('button', { name: fr.screen_pair_button() }).click();
		await p.getByText(PAIRING_CODE).waitFor();
		await audit(p, `${where} screen pairing`);
		// the invitation, in the same door (a link made for the audit, never used)
		await p.goto(invitation('door', 'Porte', false));
		await p.getByRole('heading', { level: 1, name: fr.invite_title({ name: 'Porte' }) }).waitFor();
		await audit(p, `${where} invitation`);
		await signIn(p);
		for (const [path, name] of SCREENS) {
			await visit(p, path);
			await audit(p, `${where} ${name}`);
		}
		// the session panel, opened
		await visit(p, '/');
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
await done(browser);
