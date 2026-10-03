// `bun scripts/shortcut-icons.ts`: the app shortcuts' icons (static/manifest.webmanifest
// `shortcuts`), drawn from the app's own Lucide paths (Icon.svelte) in the brand's colors:
// the cloud stroke on the petrol disc of the app icon. 96 px, the size launchers ask for.
// Run again after changing a shortcut's icon; the PNGs are committed.

import { chromium } from 'playwright';

const SHORTCUTS = { 'shortcut-leave': 'log-out', 'shortcut-night': 'moon', 'shortcut-tempo': 'zap', 'shortcut-feed': 'utensils' } as const;
const SIZE = 96;
// the manifest's theme and background colors
const PETROL = '#0e5e68';
const CLOUD = '#f2f0ea';

const root = new URL('../', import.meta.url).pathname;
const source = await Bun.file(`${root}src/lib/components/Icon.svelte`).text();
const path = (name: string) => {
	const found = source.match(new RegExp(`'${name}': '([^']+)'`));
	if (!found) throw new Error(`no icon ${name} in Icon.svelte`);
	return found[1];
};

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: SIZE, height: SIZE } });
for (const [file, icon] of Object.entries(SHORTCUTS)) {
	const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${SIZE}" height="${SIZE}" viewBox="0 0 24 24">
		<circle cx="12" cy="12" r="12" fill="${PETROL}"/>
		<g transform="translate(5.5 5.5) scale(0.54)" fill="none" stroke="${CLOUD}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${path(icon)}</g>
	</svg>`;
	await page.setContent(`<html><body style="margin:0;background:transparent">${svg}</body></html>`);
	await page.locator('svg').screenshot({ path: `${root}static/icons/${file}.png`, omitBackground: true });
	console.log(`static/icons/${file}.png (${icon})`);
}
await browser.close();
