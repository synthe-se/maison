// `bun scripts/imports.fix.ts`: Kit 3 resolves `#lib/*` through package.json `imports`, which
// maps paths verbatim, so every import names its file with its extension
// (`#lib/api.ts`, `#lib/live.svelte.ts`, `#lib/paraglide/messages.js`). Adds missing ones.

import { Glob } from 'bun';
import { existsSync } from 'node:fs';

const src = new URL('../src/', import.meta.url).pathname;
let changed = 0;
for await (const f of new Glob('**/*.{ts,svelte}').scan(src)) {
	if (f.startsWith('lib/paraglide/')) continue;
	const text = await Bun.file(src + f).text();
	const next = text.replace(/(['"])#lib\/([^'"]+)\1/g, (all, q, path: string) => {
		if (existsSync(`${src}lib/${path}`)) return all; // already names a file
		for (const ext of ['.ts', '.js', '.svelte']) if (existsSync(`${src}lib/${path}${ext}`)) return `${q}#lib/${path}${ext}${q}`;
		return all;
	});
	if (next !== text) {
		await Bun.write(src + f, next);
		changed++;
	}
}
console.log(`imports: ${changed} file(s) fixed`);
