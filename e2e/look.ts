// Not a check: screenshots of every screen, light and dark, phone and desktop, into SHOTS —
// to look at the interface with one's own eyes after a change.
import { PHONE, SCREENS, done, launch, open, shot, signIn, visit } from './lib.ts';
import { house } from './house.ts';

const browser = await launch();
for (const scheme of ['light', 'dark'] as const) {
	for (const [label, viewport] of [
		['phone', PHONE],
		['desktop', { width: 1440, height: 1000 }]
	] as const) {
		const p = await open(browser, { colorScheme: scheme, viewport, deviceScaleFactor: 2 });
		await house().serve(p);
		await signIn(p);
		for (const [path, name] of SCREENS) {
			await visit(p, path);
			await shot(p, `${scheme}-${label}-${name}`, true);
		}
		await p.context().close();
	}
}
await done(browser);
