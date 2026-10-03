// Not a check: the README's picture. The dashboard of an invented, well-stocked house (the
// e2e house, plus every family the dashboard shows), signed in like any scenario: nothing of
// the real flat, no login to keep alive, the same image every time. scripts/screenshot.sh
// runs it and passes OUT (the JPEG to write).
import { BASE, done, launch, open, settled, signIn } from './lib.ts';
import { house } from './house.ts';
import { showcase } from './showcase.ts';

const OUT = process.env.OUT ?? 'shots/readme.jpg';

const browser = await launch();
const p = await open(browser, { colorScheme: 'light', viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
await house().serve(p);
await showcase(p);
await signIn(p);
await p.goto(BASE + '/');
await p.locator('main h1').first().waitFor();
// every group has answered (no skeleton left), then the fonts and the last paint
await settled(p);
await p.screenshot({ path: OUT, fullPage: true, type: 'jpeg', quality: 90 });
console.log(`Saved -> ${OUT}`);
await done(browser);
