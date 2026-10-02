// `bun run check`: corners and control heights come from the scales in
// src/styles/tokens.css (--radius-*, --control-h*), not from literals in a component.
// Allowed radii: a token, 0, 50% (a circle), inherit.

import { Glob } from 'bun';

const src = new URL('../src/', import.meta.url).pathname;
const RADIUS = /border(?:-[a-z]+)*-radius\s*:\s*([^;}"]+)/g;
const RADIUS_OK = /^(?:(?:var\(--radius[a-z0-9-]*\)|0|50%|inherit)\s*)+$/;
const CONTROL = /min-height\s*:\s*(36|38|40|44)px/g;
const problems: string[] = [];

for await (const f of new Glob('**/*.{svelte,css}').scan(src)) {
	if (f === 'styles/tokens.css' || f.startsWith('lib/paraglide/')) continue;
	const text = await Bun.file(src + f).text();
	const line = (i: number) => text.slice(0, i).split('\n').length;
	for (const m of text.matchAll(RADIUS)) {
		if (!RADIUS_OK.test(m[1].trim())) problems.push(`src/${f}:${line(m.index)}: border-radius ${m[1].trim()}: use a --radius-* token`);
	}
	for (const m of text.matchAll(CONTROL)) {
		problems.push(`src/${f}:${line(m.index)}: min-height ${m[1]}px: use --control-h, --control-h-s or --control-h-xs`);
	}
}

if (problems.length) {
	console.error(`css: ${problems.length} problem(s)\n  ${problems.join('\n  ')}`);
	process.exit(1);
}
console.log('css: radii and control heights from the tokens');
