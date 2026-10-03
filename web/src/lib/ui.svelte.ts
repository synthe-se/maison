// What the interface remembers about itself: the theme, the toasts, and the two live regions
// that say each outcome once (the layout mounts them, only their text changes).

import { haptic, FAILURE } from '#lib/haptics.ts';
import { errorText } from '#lib/errors.ts';
import { stored, text as words } from '#lib/stored.ts';
import { PUBLIC_THEME_KEY } from '$app/env/public';

export type Theme = 'system' | 'light' | 'dark';

/** A toast's one action (« Rétablir » a deleted meal): its words, what it does, and where the
 * focus goes when the toast leaves while holding it (the row it was about is gone). */
export interface ToastAction {
	label: string;
	run: () => unknown;
	back?: () => HTMLElement | null | undefined;
}

export interface ToastOptions {
	/** A failure: assertive, and it stays until dismissed. */
	warn?: boolean;
	action?: ToastAction;
}

type Toast = { id: number; text: string; warn: boolean; action?: ToastAction };

/** How long a toast stays once nothing holds it (WCAG 2.2.1); one with an action longer
 * (Material snackbar: 4 to 10 s), the time to reach it. */
export const TOAST_MS = 5_000;
export const ACTION_TOAST_MS = 10_000;

// « system » is nothing kept: the page follows the system
const keptTheme = stored<Theme>(PUBLIC_THEME_KEY, 'system', words<Theme>(['light', 'dark']));

class Ui {
	theme = $state<Theme>(keptTheme.get());
	toasts = $state<Toast[]>([]);
	polite = $state('');
	assertive = $state('');
	#next = 0;
	#timers = new Map<number, ReturnType<typeof setTimeout>>();

	setTheme(next: Theme) {
		this.theme = next;
		keptTheme.set(next === 'system' ? undefined : next);
	}

	/** Say something politely, once (a screen reader hears it; nothing moves on screen). */
	say(text: string) {
		this.polite = '';
		queueMicrotask(() => (this.polite = text));
	}

	/** A short message under the fingers, read once. Errors stay until dismissed. Returns its
	 * id (its action button is `toast-action-<id>`). */
	toast(text: string, { warn = false, action }: ToastOptions = {}): number {
		// the same words again replace the toast already saying them (a failure repeated by a
		// poll or a search does not stack up errors that never leave)
		for (const t of this.toasts) if (t.text === text && t.warn === warn) this.dismiss(t.id);
		const id = this.#next++;
		this.toasts.push({ id, text, warn, action });
		if (warn) {
			this.assertive = '';
			queueMicrotask(() => (this.assertive = text));
		} else {
			this.say(text);
			this.resume(id);
		}
		return id;
	}

	/** A gesture failed: felt, said in plain words (errors.ts), and shown until read. */
	fail(error: unknown) {
		haptic(FAILURE);
		this.toast(errorText(error), { warn: true });
	}

	dismiss(id: number) {
		clearTimeout(this.#timers.get(id));
		this.#timers.delete(id);
		this.toasts = this.toasts.filter((t) => t.id !== id);
	}

	/** Hover or focus holds a toast; leaving lets it go after its time (WCAG 2.2.1). */
	pause(id: number) {
		clearTimeout(this.#timers.get(id));
	}

	resume(id: number) {
		const t = this.toasts.find((x) => x.id === id);
		if (!t || t.warn) return;
		clearTimeout(this.#timers.get(id));
		this.#timers.set(
			id,
			setTimeout(() => this.dismiss(id), t.action ? ACTION_TOAST_MS : TOAST_MS)
		);
	}
}

export const ui = new Ui();
