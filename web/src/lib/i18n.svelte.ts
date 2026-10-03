// Locale switching without reload, and the reader's formats.

import { m } from '#lib/paraglide/messages.js';
import {
	getLocale as resolveLocale,
	isLocale,
	locales,
	overwriteGetLocale,
	setLocale as persistLocale,
	type Locale
} from '#lib/paraglide/runtime.js';

const current = $state<{ locale: Locale }>({ locale: resolveLocale() });
overwriteGetLocale(() => current.locale);
if (typeof document !== 'undefined') document.documentElement.lang = current.locale;

export { locales, isLocale };
export type { Locale };

export const locale = (): Locale => current.locale;

export function switchLocale(next: Locale) {
	if (next === current.locale) return;
	persistLocale(next, { reload: false });
	current.locale = next;
	document.documentElement.lang = next;
}

/** A language named in itself, from its own message file. */
export function localeName(l: Locale): string {
	return m.locale_name({}, { locale: l });
}

/** A number in the reader's format (« 1 234,5 » in French). */
export function num(n: number, digits = 0): string {
	return new Intl.NumberFormat(current.locale, { maximumFractionDigits: digits, minimumFractionDigits: digits }).format(n);
}

/** A share in the reader's format: 0.62 → « 62 % » (narrow no-break space in French, from Intl). */
export function percent(share: number): string {
	return new Intl.NumberFormat(current.locale, { style: 'percent', maximumFractionDigits: 0 }).format(share);
}

/** A date in the reader's words. */
export function date(d: Date | number, opts: Intl.DateTimeFormatOptions): string {
	return new Intl.DateTimeFormat(current.locale, opts).format(d);
}

/** A time of day in the reader's format (« 18:02 », « 6:02 PM »). */
export function clock(d: Date | number): string {
	return date(d, { hour: 'numeric', minute: '2-digit' });
}

/** « 08:00 » as a device or the backend writes a time of day → the reader's format. */
export function hhmm(text: string): string {
	return clock(new Date(2000, 0, 1, Number(text.slice(0, 2)), Number(text.slice(3, 5))));
}

/** A date in full (« 12 novembre 2026 »): an ISO day (« 2026-11-12 », a local day) or a moment. */
export function longDate(d: string | Date | number): string {
	return date(typeof d === 'string' ? localDay(d) : d, { day: 'numeric', month: 'long', year: 'numeric' });
}

/** Words joined as the reader lists them (« lun., mer. et ven. »). */
export function list(items: string[], style: Intl.ListFormatStyle = 'short'): string {
	return new Intl.ListFormat(current.locale, { style }).format(items);
}

/** A word inside a sentence (« bleu », « eau basse »): lower case in the reader's rules. */
export function lower(text: string): string {
	return text.toLocaleLowerCase(current.locale);
}

/** A sentence's first word inside another sentence (« Fontaine : eau basse »). */
export function inSentence(text: string): string {
	return text.charAt(0).toLocaleLowerCase(current.locale) + text.slice(1);
}

/** The window's title: the page's, then Maison's (« Salon · Maison »); Maison's alone at home. */
export function pageTitle(title?: string): string {
	const brand = m.branding_name();
	return !title || title === brand ? brand : `${title} · ${brand}`;
}

/** When something happened: the time if today, else the day and the time (« 3 oct. 21:04 »). */
export function when(d: Date | number | string): string {
	const at = new Date(d);
	return new Date().toDateString() === at.toDateString()
		? clock(at)
		: date(at, { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' });
}

// ── calendar days, as devices and RTE write them (« 2026-11-12 »): local days, never UTC ──

/** « 2026-11-12 » → that day at local midnight (`new Date(iso)` would be UTC midnight). */
export function localDay(iso: string): Date {
	const [y, mo, d] = iso.slice(0, 10).split('-').map(Number);
	return new Date(y, mo - 1, d);
}

/** A local day written « 2026-11-12 ». */
export function isoDay(d: Date): string {
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** The local day `offset` days from today, written « 2026-11-12 ». */
export function dayFromToday(offset: number): string {
	const d = new Date();
	d.setDate(d.getDate() + offset);
	return isoDay(d);
}

/** « Aujourd’hui », « Demain », else the weekday in full (« vendredi »). */
export function dayLabel(iso: string): string {
	if (iso === dayFromToday(0)) return m.day_today();
	if (iso === dayFromToday(1)) return m.day_tomorrow();
	return date(localDay(iso), { weekday: 'long' });
}

/** « jeu. 12 nov. » */
export function shortDay(iso: string): string {
	return date(localDay(iso), { weekday: 'short', day: 'numeric', month: 'short' });
}

/** « mardi 12 novembre » */
export function longDay(iso: string): string {
	return date(localDay(iso), { weekday: 'long', day: 'numeric', month: 'long' });
}

/** The name of a weekday, 0 = Monday (ISO order): « lundi », « lun. ». */
export function weekday(index: number, form: 'long' | 'short'): string {
	// 3 January 2000 was a Monday
	return date(new Date(2000, 0, 3 + index), { weekday: form });
}
