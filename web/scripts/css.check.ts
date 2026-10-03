// `bun run check`: what a style says comes from the scales in src/styles/tokens.css, not from
// literals in a component or in app.css:
// - corners (--radius-*): a token, 0, 50% (a circle), inherit;
// - control heights (--control-h*);
// - font sizes: never in px (a token, `--t-*`, or rem that scales with the reader's setting);
// - colors: no rgb()/hsl() nor hex outside tokens.css (a token, color-mix of tokens);
// - the focus ring: `outline` and `outline-offset` from --focus-ring / --focus-offset /
//   --mark-ring (or none, 0).
// In a .svelte file only its <style> blocks are read (markup ids and hrefs are not colors).

import { Glob } from 'bun';

const src = new URL('../src/', import.meta.url).pathname;
const TOKENS = 'styles/tokens.css';

type Rule = { re: RegExp; ok?: RegExp; say: (m: RegExpMatchArray) => string };
const RULES: Rule[] = [
	{
		re: /border(?:-[a-z]+)*-radius\s*:\s*([^;}"]+)/g,
		ok: /^(?:(?:var\(--radius[a-z0-9-]*\)|0|50%|inherit)\s*)+$/,
		say: (m) => `border-radius ${m[1].trim()}: use a --radius-* token`
	},
	{
		re: /min-height\s*:\s*(36|38|40|44|52)px/g,
		say: (m) => `min-height ${m[1]}px: use --control-h, --control-h-s, --control-h-xs or --control-h-l`
	},
	{ re: /font-size\s*:\s*[\d.]+px/g, say: (m) => `${m[0]}: font sizes in px do not follow the reader's setting: use a --t-* token` },
	{ re: /\bfont\s*:\s*[^;}]*\b[\d.]+px/g, say: (m) => `${m[0]}: a font in px: use a --t-* token` },
	{ re: /\b(?:rgba?|hsla?)\(/g, say: (m) => `${m[0]}…: a color literal: use a token from tokens.css` },
	{ re: /(?<![\w-])#[0-9a-fA-F]{3,8}\b/g, say: (m) => `${m[0]}: a color literal: use a token from tokens.css` },
	{
		re: /\boutline(?:-offset)?\s*:\s*([^;}]+)/g,
		ok: /^(?:none|0|var\(--(?:focus-ring|focus-offset|mark-ring)\))$/,
		say: (m) => `${m[0].trim()}: use --focus-ring / --focus-offset (or --mark-ring)`
	}
];

/** The CSS of a file: a stylesheet whole, a component's <style> blocks (positions kept). */
function cssOf(file: string, text: string): string {
	if (file.endsWith('.css')) return text;
	// blank out everything outside <style>…</style>, keeping the line breaks for the positions
	let out = '';
	let at = 0;
	for (const m of text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
		const start = m.index + m[0].indexOf(m[1]);
		out += text.slice(at, start).replace(/[^\n]/g, ' ') + m[1];
		at = start + m[1].length;
	}
	return out + text.slice(at).replace(/[^\n]/g, ' ');
}

const problems: string[] = [];
for await (const f of new Glob('**/*.{svelte,css}').scan(src)) {
	if (f === TOKENS || f.startsWith('lib/paraglide/')) continue;
	const css = cssOf(f, await Bun.file(src + f).text());
	const line = (i: number) => css.slice(0, i).split('\n').length;
	for (const rule of RULES) {
		for (const m of css.matchAll(rule.re)) {
			if (rule.ok?.test((m[1] ?? '').trim())) continue;
			problems.push(`src/${f}:${line(m.index)}: ${rule.say(m)}`);
		}
	}
}

if (problems.length) {
	console.error(`css: ${problems.length} problem(s)\n  ${problems.join('\n  ')}`);
	process.exit(1);
}
console.log('css: radii, control heights, font sizes, colors and focus rings from the tokens');
