// `bun run check`: what a style says comes from the scales in src/styles/tokens.css, not from
// literals in a component or in app.css:
// - corners (--radius-*): a token, 0, 50% (a circle), inherit;
// - control heights (--control-h*): no height, min-height nor max-height of a control's px;
// - font sizes: never in px (a token, `--t-*`, or rem that scales with the reader's setting);
// - colors: no rgb()/hsl() nor hex outside tokens.css (a token, color-mix of tokens), and the
//   raw palette (--raw-*) only there: components use its meanings (--accent, --brand…);
// - the focus ring: `outline` and `outline-offset` from --focus-ring / --focus-offset /
//   --mark-ring (or none, 0);
// - layers: z-index from --z-* (or 0, auto);
// - durations: a transition's or an animation's from --dur-*;
// - opacities: from --*-opacity (or 0, 1);
// - breakpoints: an @media width is one of tokens.css's (custom properties cannot be read there).
// In a .svelte file its <style> blocks are read, and its `style:` directives and `style`
// attributes as declarations (markup ids and hrefs are not colors).

import { Glob } from 'bun';

/** The widths a media query may name (docs/ux.md « Layout »; said in tokens.css). */
const BREAKPOINTS = { min: [600, 840, 1200, 1600], max: [599, 839, 1199, 1599] } as const;

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
		re: /(?<![\w-])((?:min-|max-)?height)\s*:\s*(32|36|38|40|44|52)px/g,
		say: (m) => `${m[1]} ${m[2]}px: use --control-h, --control-h-s, --control-h-xs or --control-h-l`
	},
	{ re: /font-size\s*:\s*[\d.]+px/g, say: (m) => `${m[0]}: font sizes in px do not follow the reader's setting: use a --t-* token` },
	{ re: /\bfont\s*:\s*[^;}]*\b[\d.]+px/g, say: (m) => `${m[0]}: a font in px: use a --t-* token` },
	{ re: /\b(?:rgba?|hsla?)\(/g, say: (m) => `${m[0]}…: a color literal: use a token from tokens.css` },
	{ re: /(?<![\w-])#[0-9a-fA-F]{3,8}\b/g, say: (m) => `${m[0]}: a color literal: use a token from tokens.css` },
	{
		re: /\boutline(?:-offset)?\s*:\s*([^;}]+)/g,
		ok: /^(?:none|0|var\(--(?:focus-ring|focus-offset|mark-ring)\))$/,
		say: (m) => `${m[0].trim()}: use --focus-ring / --focus-offset (or --mark-ring)`
	},
	{ re: /var\(--raw-[a-z0-9-]+\)/g, say: (m) => `${m[0]}: the raw palette stays in tokens.css: use what it means (--accent, --brand…)` },
	{
		re: /\bz-index\s*:\s*([^;}]+)/g,
		ok: /^(?:var\(--z-[a-z]+\)|0|auto)$/,
		say: (m) => `${m[0].trim()}: use a --z-* layer`
	},
	{
		re: /\b(?:transition|animation)(?:-duration|-delay)?\s*:\s*([^;}]+)/g,
		ok: /^(?:(?!(?:^|[\s,(])[\d.]+m?s\b).)*$/,
		say: (m) => `${m[0].trim()}: a duration literal: use a --dur-* token`
	},
	{
		re: /(?<![\w-])opacity\s*:\s*([^;}]+)/g,
		ok: /^(?:var\(--[a-z]+-opacity\)|0|1)$/,
		say: (m) => `${m[0].trim()}: use an --*-opacity token`
	},
	{
		re: /@media[^{]*?\((min|max)-width\s*:\s*([^)]+)\)/g,
		ok: undefined,
		say: (m) =>
			`${m[0].trim()}: a breakpoint of tokens.css: min-width ${BREAKPOINTS.min.join('/')} px or max-width ${BREAKPOINTS.max.join('/')} px`
	}
];

/** A media query's width is one of the breakpoints. */
const breakpoint = (m: RegExpMatchArray) =>
	(BREAKPOINTS[m[1] as 'min' | 'max'] as readonly number[]).map((n) => `${n}px`).includes(m[2].trim());

/** The CSS of a file: a stylesheet whole; a component's <style> blocks, and its markup's
 * `style:prop={…}` directives and `style="…"` attributes as declarations (positions kept). */
function cssOf(file: string, text: string): string {
	if (file.endsWith('.css')) return text;
	// blank out everything else, keeping the line breaks for the positions
	const keep: { start: number; text: string }[] = [];
	for (const m of text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) keep.push({ start: m.index + m[0].indexOf(m[1]), text: m[1] });
	for (const m of text.matchAll(/\bstyle:([a-z-]+)=("[^"]*"|\{[^}]*\})/g))
		keep.push({ start: m.index, text: `${m[1]}: ${m[2].slice(1, -1)};` });
	for (const m of text.matchAll(/\sstyle="([^"]*)"/g)) keep.push({ start: m.index + 8, text: `${m[1]};` });
	let out = text.replace(/[^\n]/g, ' ');
	for (const k of keep) out = out.slice(0, k.start) + k.text.replace(/\n/g, '\n') + out.slice(k.start + k.text.length);
	return out;
}

const problems: string[] = [];
for await (const f of new Glob('**/*.{svelte,css}').scan(src)) {
	if (f === TOKENS || f.startsWith('lib/paraglide/')) continue;
	const css = cssOf(f, await Bun.file(src + f).text());
	const line = (i: number) => css.slice(0, i).split('\n').length;
	for (const rule of RULES) {
		for (const m of css.matchAll(rule.re)) {
			if (rule.ok?.test((m[1] ?? '').trim())) continue;
			if (rule.re.source.startsWith('@media') && breakpoint(m)) continue;
			problems.push(`src/${f}:${line(m.index)}: ${rule.say(m)}`);
		}
	}
}

if (problems.length) {
	console.error(`css: ${problems.length} problem(s)\n  ${problems.join('\n  ')}`);
	process.exit(1);
}
console.log('css: radii, control heights, font sizes, colors, focus rings, layers, durations, opacities and breakpoints from the tokens');
