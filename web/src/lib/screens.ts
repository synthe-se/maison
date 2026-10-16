// A wall screen (the frame: docs/passkeys.md « Wall screens »): paired once by an admin from the
// code it shows, then in by an HttpOnly cookie the backend sets. The page never holds a token:
// it keeps the pairing's secret in memory while it waits, and that is all.

import { del, get, path, post } from '#lib/api.ts';
import { withRecentPasskey } from '#lib/passkeys.ts';
import { isLocale, switchLocale } from '#lib/i18n.svelte.ts';
import { source } from '#lib/live.svelte.ts';
import { isTheme, ui } from '#lib/ui.svelte.ts';

/** A pairing, as the screen shows it: the code, the secret it claims with, how long it waits
 * and how often it asks. */
export interface Pairing {
	code: string;
	secret: string;
	expiresInMs: number;
	pollMs: number;
}

/** A paired screen, as an admin lists it. */
export interface Screen {
	id: string;
	name: string;
	createdMs: number;
	lastUsedMs: number | null;
}

export const screensApi = {
	/** Signed out: this browser asks to become a screen. */
	start: () => post<Pairing>('/screens/pairing'),
	/** Whether an admin paired it: once paired, the backend has set its cookie. */
	claim: (secret: string) => post<{ paired: boolean; screen?: { name: string } }>('/screens/pairing/claim', { secret }),
	/** This browser as a screen, at each start of the page: its cookie renewed (a POST: a
	 * change); refused when it is none. */
	me: () => post<{ screen: { name: string } }>('/screens/me'),
	/** An admin pairs the screen showing `code`, from the screen's own network, after a recent
	 * passkey sign-in (a paired screen drives the house for months). */
	pair: (code: string, name: string) => withRecentPasskey(() => post<{ screen: { name: string } }>('/screens', { code, name })),
	list: () => get<{ screens: Screen[] }>('/screens').then((r) => r.screens),
	remove: (id: string) => del<void>(path`/screens/${id}`)
};

// what the account page reads; it changes only from that page, which reads it again
export const screensData = source('account:screens', screensApi.list);

/** What a screen's address names, taken and wiped from the address: its language (`#lang=fr`: a
 * wall screen's browser may say another, the frame's WPE says en-US whatever its system), and its
 * theme (`theme=sun`: nobody is there to choose one). Nothing secret rides there. */
export function takeAddress(location: Location = window.location, history: History = window.history) {
	const fragment = new URLSearchParams(location.hash.slice(1));
	const lang = fragment.get('lang');
	const theme = fragment.get('theme');
	if (lang === null && theme === null) return;
	if (lang !== null && isLocale(lang)) switchLocale(lang);
	if (theme !== null && isTheme(theme)) ui.setTheme(theme);
	history.replaceState(history.state, '', location.pathname + location.search);
}
