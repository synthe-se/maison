// Locale switching without reload, and the reader's formats: numbers, dates and times, lists,
// durations. Whatever depends on today reads the shared clock (`time.now`): it rolls over at
// midnight, and a test sets the moment it needs.

import { m } from '#lib/paraglide/messages.js';
import { time } from '#lib/clock.svelte.ts';
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
	return time.now.toDateString() === at.toDateString()
		? clock(at)
		: date(at, { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' });
}

// ── calendar days, as devices and RTE write them (« 2026-11-12 »): local days, never UTC ──

/** « 2026-11-12 » → that day at local midnight (`new Date(iso)` would be UTC midnight). */
function localDay(iso: string): Date {
	const [y, mo, d] = iso.slice(0, 10).split('-').map(Number);
	return new Date(y, mo - 1, d);
}

/** A local day written « 2026-11-12 ». */
export function isoDay(d: Date): string {
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** The local day `offset` days from today, written « 2026-11-12 ». */
export function dayFromToday(offset: number): string {
	const d = new Date(time.now);
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

// ── durations: « 45 s », « 3 min 20 s », « 1 h 30 », « 2 j 4 h »; two units at most, the units
// from the messages ──

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A duration given in seconds. */
export function formatDuration(seconds: number): string {
	const t = Math.max(0, Math.round(seconds));
	if (t < MINUTE) return m.duration_seconds({ s: t });
	if (t < HOUR) {
		const min = Math.floor(t / MINUTE);
		const s = t % MINUTE;
		return s === 0 ? m.duration_minutes({ m: min }) : m.duration_minutes_seconds({ m: min, s });
	}
	if (t < DAY) {
		const h = Math.floor(t / HOUR);
		const min = Math.floor((t % HOUR) / MINUTE);
		return min === 0 ? m.duration_hours({ h }) : m.duration_hours_minutes({ h, m: min, mm: String(min).padStart(2, '0') });
	}
	const d = Math.floor(t / DAY);
	const h = Math.floor((t % DAY) / HOUR);
	return h === 0 ? m.duration_days({ d }) : m.duration_days_hours({ d, h });
}

/** A duration given in minutes (device counters). */
export function formatMinutes(minutes: number): string {
	return formatDuration(minutes * MINUTE);
}

/**
 * Since when a device is out of reach, for every tile alike (docs/ux.md § 4):
 * « Injoignable depuis 12 min » within the hour, then the time it was last heard
 * (« Injoignable depuis 14:20 », with the day when not today), or just « Injoignable »
 * when it never answered. `at`: ISO text or ms since the epoch; 0 or null is never.
 */
export function unreachableSince(at: string | number | null | undefined): string {
	if (!at) return m.state_unreachable();
	const ms = typeof at === 'number' ? at : new Date(at).getTime();
	const minutes = Math.max(1, Math.round((time.now.getTime() - ms) / 60_000));
	return minutes < HOUR / MINUTE
		? m.state_unreachable_for({ duration: formatMinutes(minutes) })
		: m.state_unreachable_since({ when: when(ms) });
}
