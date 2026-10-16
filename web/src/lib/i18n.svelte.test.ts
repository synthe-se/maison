import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import {
	clock,
	date,
	dayFromToday,
	dayLabel,
	hhmm,
	inSentence,
	isoDay,
	isLocale,
	locale,
	localeName,
	list,
	locales,
	longDate,
	longDay,
	lower,
	num,
	pageTitle,
	percent,
	shortDay,
	switchLocale,
	weekday,
	when,
	facts,
	formatDuration,
	formatMinutes,
	unreachableSince
} from './i18n.svelte.ts';
import { time } from './clock.svelte.ts';

const NNBSP = '\u202f';

// the browser's language picks the first locale; these tests read French, and leave the page
// as they found it (the choice is persisted in localStorage)
const initial = locale();
const startLang = document.documentElement.lang;
beforeAll(() => switchLocale('fr'));
afterAll(() => {
	switchLocale(initial);
	localStorage.removeItem('maison-locale');
});

describe('locale', () => {
	afterEach(() => switchLocale('fr'));

	it('says the starting locale on <html>', () => {
		expect(startLang).toBe(initial);
		expect(document.documentElement.lang).toBe(locale());
		expect(locales).toContain('en');
		expect(isLocale('en')).toBe(true);
		expect(isLocale('de')).toBe(false);
	});

	it('switches without reload: messages, formats and <html lang> follow', () => {
		switchLocale('en');
		expect(locale()).toBe('en');
		expect(document.documentElement.lang).toBe('en');
		expect(m.nav_home()).toBe('Home');
		expect(num(1234.5, 1)).toBe('1,234.5');
		switchLocale('fr');
		expect(m.nav_home()).toBe('Accueil');
		expect(document.documentElement.lang).toBe('fr');
	});

	it('switching to the current locale changes nothing', () => {
		const set = vi.spyOn(Storage.prototype, 'setItem');
		switchLocale('fr');
		expect(set).not.toHaveBeenCalled();
	});

	it('names each language in itself', () => {
		expect(localeName('fr')).toBe('Français');
		expect(localeName('en')).toBe('English');
	});
});

describe('formats (French)', () => {
	afterEach(() => vi.useRealTimers());

	it('writes numbers the French way, with the asked digits', () => {
		expect(num(1234.5, 1)).toBe(`1${NNBSP}234,5`);
		expect(num(2.345)).toBe('2');
		expect(num(2, 2)).toBe('2,00');
	});

	it('writes a share as a percentage, the sign held to its number', () => {
		// a no-break space (narrow or not, as the browser's Intl data has it), never a breaking one
		expect(percent(0.62)).toMatch(/^62[\u00a0\u202f]%$/);
		expect(percent(0.005)).toMatch(/^1[\u00a0\u202f]%$/);
	});

	it('writes a time of day on 24 h', () => {
		expect(clock(new Date(2026, 9, 2, 18, 2))).toBe('18:02');
		expect(date(new Date(2026, 9, 2), { month: 'long' })).toBe('octobre');
	});

	it('says only the time for today, the day and time otherwise', () => {
		time.now = new Date(2026, 9, 2, 21, 0);
		expect(when(new Date(2026, 9, 2, 18, 2))).toBe('18:02');
		expect(when(new Date(2026, 9, 2, 18, 2).toISOString())).toBe('18:02');
		const other = when(new Date(2026, 9, 1, 21, 4).getTime());
		expect(other).toContain('1 oct.');
		expect(other).toContain('21:04');
	});
});

describe('calendar days', () => {
	afterEach(() => vi.useRealTimers());

	it('reads « 2026-11-12 » as a local day, not UTC', () => {
		expect(longDate('2026-11-12T23:00:00Z')).toBe('12 novembre 2026');
		expect(shortDay('2026-11-12')).toBe('jeu. 12 nov.');
	});

	it('writes a local day back, zero-padded', () => {
		expect(isoDay(new Date(2026, 0, 5, 23, 30))).toBe('2026-01-05');
	});

	it('counts days from today across a month end', () => {
		time.now = new Date(2026, 9, 31, 22, 0);
		expect(dayFromToday(0)).toBe('2026-10-31');
		expect(dayFromToday(1)).toBe('2026-11-01');
		expect(dayFromToday(-31)).toBe('2026-09-30');
	});

	it('says today, tomorrow, else the weekday', () => {
		time.now = new Date(2026, 9, 2, 9, 0); // a Friday
		expect(dayLabel('2026-10-02')).toBe(m.day_today());
		expect(dayLabel('2026-10-03')).toBe(m.day_tomorrow());
		expect(dayLabel('2026-10-05')).toBe('lundi');
	});

	it('writes short and long days', () => {
		expect(shortDay('2026-11-12')).toBe('jeu. 12 nov.');
		expect(longDay('2026-11-12')).toBe('jeudi 12 novembre');
	});

	it('names weekdays from Monday', () => {
		expect(weekday(0, 'long')).toBe('lundi');
		expect(weekday(6, 'long')).toBe('dimanche');
		expect(weekday(0, 'short')).toBe('lun.');
	});

	it('writes a device’s « 19:30 » in the reader’s time', () => {
		expect(hhmm('08:05')).toBe(clock(new Date(2000, 0, 1, 8, 5)));
		expect(hhmm('19:30')).toBe('19:30');
	});

	it('writes a date in full, from an ISO day (a local day, never UTC) or a moment', () => {
		expect(longDate('2026-08-01')).toBe('1 août 2026');
		expect(longDate(new Date(2026, 9, 2, 23, 30))).toBe('2 octobre 2026');
	});

	it('lists words as the reader does, and puts a word in a sentence', () => {
		expect(list(['lun.', 'mer.', 'ven.'])).toBe('lun., mer. et ven.');
		expect(lower('Bleu')).toBe('bleu');
		expect(inSentence('Eau basse')).toBe('eau basse');
	});

	it('titles the window: the page, then Maison; Maison alone at home', () => {
		expect(pageTitle('Salon')).toBe(`Salon · ${m.branding_name()}`);
		expect(pageTitle(m.branding_name())).toBe(m.branding_name());
		expect(pageTitle()).toBe(m.branding_name());
	});
});

describe('formatDuration', () => {
	it('says seconds under a minute, rounded, never negative', () => {
		expect(formatDuration(45)).toBe(m.duration_seconds({ s: 45 }));
		expect(formatDuration(12.6)).toBe(m.duration_seconds({ s: 13 }));
		expect(formatDuration(-5)).toBe(m.duration_seconds({ s: 0 }));
	});

	it('says minutes, with the seconds only when there are some', () => {
		expect(formatDuration(180)).toBe(m.duration_minutes({ m: 3 }));
		expect(formatDuration(200)).toBe(m.duration_minutes_seconds({ m: 3, s: 20 }));
	});

	it('says hours, with the minutes padded to two digits', () => {
		expect(formatDuration(7200)).toBe(m.duration_hours({ h: 2 }));
		expect(formatDuration(3600 + 5 * 60)).toBe(m.duration_hours_minutes({ h: 1, m: 5, mm: '05' }));
		expect(formatDuration(3600 + 5 * 60)).toMatch(/^1\sh\s05$/);
	});

	it('says days, with the hours only when there are some; two units at most', () => {
		expect(formatDuration(86_400)).toBe(m.duration_days({ d: 1 }));
		expect(formatDuration(2 * 86_400 + 4 * 3600 + 59)).toBe(m.duration_days_hours({ d: 2, h: 4 }));
	});
});

describe('formatMinutes', () => {
	it('reads a counter in minutes', () => {
		expect(formatMinutes(90)).toBe(formatDuration(5400));
		expect(formatMinutes(0.5)).toBe(m.duration_seconds({ s: 30 }));
	});
});

describe('unreachableSince', () => {
	it('within the hour: « depuis 12 min »; then the time it was last heard', () => {
		const now = time.now.getTime();
		expect(unreachableSince(now - 12 * 60_000)).toBe(m.state_unreachable_for({ duration: m.duration_minutes({ m: 12 }) }));
		expect(unreachableSince(new Date(now - 30_000).toISOString())).toBe(
			m.state_unreachable_for({ duration: m.duration_minutes({ m: 1 }) })
		);
		const old = now - 3 * 3_600_000;
		expect(unreachableSince(old)).toBe(m.state_unreachable_since({ when: when(old) }));
	});

	it('never heard: says so', () => {
		expect(unreachableSince(0)).toBe(m.state_unreachable());
		expect(unreachableSince(null)).toBe(m.state_unreachable());
	});
});

describe('facts', () => {
	it('puts facts side by side, the empty ones left out', () => {
		expect(facts('42 W', null, '', false, '0,7 c€/h')).toBe(`42 W${m.facts_separator()}0,7 c€/h`);
		expect(facts('seul')).toBe('seul');
	});
});
