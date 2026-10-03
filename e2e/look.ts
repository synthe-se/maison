// Not a check: screenshots of every screen, light and dark, phone and desktop, into SHOTS —
// to look at the interface with one's own eyes after a change.
import { BASE, PHONE, done, launch, open, settled, shot, signIn } from './lib.ts';
import { house } from './house.ts';

const SCREENS = ['/', '/remote', '/tempo-predictions', '/device/feeder-1', '/hue-lamp/hue-01', '/meross/plug-1', '/tv', '/androidtv'];
const browser = await launch();
for (const scheme of ['light', 'dark'] as const) {
	for (const [label, viewport] of [
		['phone', PHONE],
		['desktop', { width: 1440, height: 1000 }]
	] as const) {
		const p = await open(browser, { colorScheme: scheme, viewport, deviceScaleFactor: 2 });
		await house().serve(p);
		await signIn(p);
		for (const path of SCREENS) {
			await p.goto(BASE + path);
			await p.locator('main h1').first().waitFor();
			await settled(p);
			await shot(p, `${scheme}-${label}${path === '/' ? '-home' : path.replaceAll('/', '-')}`, true);
		}
		await p.context().close();
	}
}
await done(browser);
