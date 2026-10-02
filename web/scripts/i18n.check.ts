// `bun run check`: the message files say each thing once, in every locale, and every key is
// said somewhere. Paraglide falls back to the base locale silently and keeps unused keys.
// Used means `m.key` / `m['key']` in web/src. Duplicates: two keys with the same text in every
// locale; a legitimate one (same words, another meaning) goes in i18n.allow.json, with its reason.

import { Glob } from 'bun';

const root = new URL('../../', import.meta.url).pathname;
const settings = await Bun.file(`${root}i18n/project.inlang/settings.json`).json();
const locales: string[] = settings.locales;
const files: Record<string, Record<string, unknown>> = {};
for (const l of locales) {
	const { $schema: _, ...messages } = await Bun.file(`${root}i18n/messages/${l}.json`).json();
	files[l] = messages;
}
const keys = new Set(locales.flatMap((l) => Object.keys(files[l])));
const problems: string[] = [];

for (const k of keys) {
	const missing = locales.filter((l) => !(k in files[l]));
	if (missing.length) problems.push(`${k}: missing in ${missing.join(', ')}`);
}

const used = new Set<string>();
for await (const f of new Glob('web/src/**/*.{ts,svelte}').scan(root)) {
	if (f.includes('/lib/paraglide/')) continue;
	const s = await Bun.file(root + f).text();
	for (const x of s.matchAll(/\bm(?:\.([a-z][a-z0-9_]*)|\[['"]([a-z][a-z0-9_]*)['"]\])/g)) used.add(x[1] ?? x[2]);
}
for (const k of keys) if (!used.has(k)) problems.push(`${k}: unused`);

type Allowed = { keys: string[]; reason: string };
const allowed: Allowed[] = await Bun.file(new URL('i18n.allow.json', import.meta.url)).json();
const allowedSet = new Set(allowed.map((a) => [...a.keys].sort().join(' ')));
for (const a of allowed) {
	if (!a.reason?.trim()) problems.push(`i18n.allow.json: ${a.keys.join(', ')} needs a reason`);
	for (const k of a.keys) if (!keys.has(k)) problems.push(`i18n.allow.json: ${k} is not a key`);
}
const byText = new Map<string, string[]>();
for (const k of keys) {
	const text = JSON.stringify(locales.map((l) => files[l][k]));
	byText.set(text, [...(byText.get(text) ?? []), k]);
}
const seen = new Set<string>();
for (const same of byText.values()) {
	if (same.length < 2) continue;
	const id = [...same].sort().join(' ');
	seen.add(id);
	if (!allowedSet.has(id)) problems.push(`${same.join(', ')}: same text in every locale (merge them, or allow in i18n.allow.json)`);
}
for (const id of allowedSet) if (!seen.has(id)) problems.push(`i18n.allow.json: ${id} are no longer duplicates`);

if (problems.length) {
	console.error(`i18n: ${problems.length} problem(s)\n  ${problems.join('\n  ')}`);
	process.exit(1);
}
console.log(`i18n: ${keys.size} keys, ${locales.join(' + ')}, all used, none twice`);
