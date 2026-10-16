// What the account page's lists say of a kept thing (a passkey, a wall screen, a session) on
// its second line, once: since when it is there, then when it was last used, or never.

import { facts, longDate, when } from '#lib/i18n.svelte.ts';

interface LifeWords {
	/** « Créée le 12 novembre 2026 » */
	since: (p: { when: string }) => string;
	/** « utilisée le 3 oct. 21:04 » */
	used: (p: { when: string }) => string;
	/** « jamais utilisée » */
	never: () => string;
}

/** Since when, then when last used (or never), side by side. */
export function lifeLine(createdMs: number, lastUsedMs: number | null, words: LifeWords): string {
	return facts(words.since({ when: longDate(createdMs) }), lastUsedMs ? words.used({ when: when(lastUsedMs) }) : words.never());
}

/** The browsers a session list names, by what their user agent says (the most specific first:
 * Edge and Opera say Chrome too, Chrome says Safari). Their own names, in every language. */
const BROWSERS: [needle: string, name: string][] = [
	['Edg', 'Edge'],
	['OPR', 'Opera'],
	['SamsungBrowser', 'Samsung Internet'],
	['Firefox', 'Firefox'],
	['FxiOS', 'Firefox'],
	['CriOS', 'Chrome'],
	['Chrome', 'Chrome'],
	['Safari', 'Safari']
];

/** The browser of a user agent, or '' when none is known. */
export function browserName(userAgent: string): string {
	return BROWSERS.find(([needle]) => userAgent.includes(needle))?.[1] ?? '';
}
