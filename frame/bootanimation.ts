// The frame's boot screen: the sign-in door's identity panel (AuthShell's side: the brand's petrol,
// the sun's path faintly over the horizon, the mark in its ring, the name in Borel), full screen and
// rendered by Chromium from the app's own sources, so nothing here restates the brand: the trail is
// read from AuthShell.svelte, colours, type and radii from tokens.css, the name from fr.json.
// Frames: the identity fades in (part0, once), then stays (part1, until Android is up).
// Usage: bun frame/bootanimation.ts <out dir>   (bootanimation.sh turns and zips them)
import { createRequire } from 'node:module';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const REPO = resolve(import.meta.dir, '..');
const WEB = join(REPO, 'web');
const { chromium } = createRequire(join(WEB, 'package.json'))('playwright') as typeof import('playwright');

/** The panel as it stands (portrait): the turn for the panel's own orientation comes after. */
const WIDTH = 800;
const HEIGHT = 1280;
const FADE = 12;

const out = resolve(process.argv[2] ?? '');
const shell = readFileSync(join(WEB, 'src/lib/components/AuthShell.svelte'), 'utf8');
const trail = shell.match(/<svg class="trail"[\s\S]*?<\/svg>/)?.[0];
if (!trail) throw new Error('AuthShell.svelte: no trail svg');
const name = JSON.parse(readFileSync(join(REPO, 'i18n/messages/fr.json'), 'utf8')).branding_name as string;
const file = (path: string) => pathToFileURL(join(WEB, path)).href;

const page = `<!doctype html><html><head><meta charset="utf-8"><style>
${readFileSync(join(WEB, 'src/styles/tokens.css'), 'utf8')}
@font-face { font-family: 'Borel Display'; src: url('${file('static/fonts/BorelDisplay-Regular.woff2')}') format('woff2'); }
html, body { margin: 0; width: ${WIDTH}px; height: ${HEIGHT}px; overflow: hidden; }
body { position: relative; display: grid; place-items: center; background: var(--raw-petrol); color: var(--raw-cloud); }
.trail { position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0.16; color: var(--raw-cloud); }
.id { position: relative; display: grid; justify-items: center; gap: 24px; opacity: var(--shown, 1); }
.mark { display: block; width: 200px; height: 200px; border-radius: calc(var(--radius-2xl) * 200 / 88);
	box-shadow: 0 0 0 2px color-mix(in srgb, var(--raw-cloud) 35%, transparent); }
.name { margin: 0; font: var(--t-brand-door-wide); font-size: 6rem; }
</style></head><body>${trail}<div class="id"><img class="mark" src="${file('static/brand.svg')}" alt=""><p class="name">${name}</p></div></body></html>`;

const html = join(tmpdir(), `maison-boot-${process.pid}.html`);
writeFileSync(html, page);
const browser = await chromium.launch();
const tab = await browser.newPage({ viewport: { width: WIDTH, height: HEIGHT } });
await tab.goto(pathToFileURL(html).href);
await tab.evaluate(() => document.fonts.ready);
for (const part of ['part0', 'part1']) mkdirSync(join(out, part), { recursive: true });
// eased in: the identity comes up softly, then holds
for (let i = 0; i < FADE; i++) {
	const t = (i + 1) / FADE;
	await tab.evaluate((shown) => document.body.style.setProperty('--shown', String(shown)), 1 - (1 - t) ** 3);
	await tab.screenshot({ path: join(out, 'part0', `${String(i).padStart(3, '0')}.png`) });
}
await tab.screenshot({ path: join(out, 'part1', '000.png') });
await browser.close();
console.log(`${FADE + 1} frames, ${WIDTH}×${HEIGHT}`);
