// `bun scripts/i18n.add.ts '{"key": ["français", "English"]}'`: add or replace messages in every
// locale at once, keys kept sorted (a plural: `{"one": "…", "other": "…"}` in place of a string).
// `bun scripts/i18n.add.ts --delete key1 key2`: remove them. The order of the values is the
// order of `locales` in i18n/project.inlang/settings.json.

const root = new URL('../../i18n/', import.meta.url).pathname;
const { locales }: { locales: string[] } = await Bun.file(`${root}project.inlang/settings.json`).json();

type Text = string | { one?: string; other: string };
const plural = (forms: { one?: string; other: string }) => [
	{
		declarations: ['input count', 'local p = count: plural'],
		selectors: ['p'],
		match: { ...(forms.one ? { 'p=one': forms.one } : {}), 'p=*': forms.other }
	}
];

const [first, ...rest] = Bun.argv.slice(2);
const remove = first === '--delete' ? rest : [];
const add: Record<string, Text[]> = first === '--delete' ? {} : JSON.parse(first);

for (const [i, locale] of locales.entries()) {
	const path = `${root}messages/${locale}.json`;
	const { $schema, ...messages } = await Bun.file(path).json();
	for (const key of remove) delete messages[key];
	for (const [key, texts] of Object.entries(add)) {
		const text = texts[i];
		if (text === undefined) throw new Error(`${key}: no ${locale} text`);
		messages[key] = typeof text === 'string' ? text : plural(text);
	}
	const sorted = Object.fromEntries(Object.entries(messages).toSorted(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
	await Bun.write(path, JSON.stringify({ $schema, ...sorted }, null, '\t') + '\n');
}
console.log(`i18n: ${Object.keys(add).length || remove.length} key(s) ${remove.length ? 'removed' : 'written'}`);
