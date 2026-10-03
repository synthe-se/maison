// What the interface remembers about itself: the theme, the toasts, and the two live regions
// that say each outcome once (the layout mounts them, only their text changes).

import { haptic, FAILURE } from '#lib/haptics.ts';
import { errorText } from '#lib/errors.ts';
import { PUBLIC_THEME_KEY as THEME_KEY } from '$app/env/public';

export type Theme = 'system' | 'light' | 'dark';

type Toast = { id: number; text: string; warn: boolean };

function storedTheme(): Theme {
	try {
		const t = localStorage.getItem(THEME_KEY);
		return t === 'light' || t === 'dark' ? t : 'system';
	} catch {
		return 'system';
	}
}

class Ui {
	theme = $state<Theme>(storedTheme());
	toasts = $state<Toast[]>([]);
	polite = $state('');
	assertive = $state('');
	#next = 0;
	#timers = new Map<number, ReturnType<typeof setTimeout>>();

	setTheme(theme: Theme) {
		this.theme = theme;
		try {
			if (theme === 'system') localStorage.removeItem(THEME_KEY);
			else localStorage.setItem(THEME_KEY, theme);
		} catch {
			// private mode: the choice lasts for the visit
		}
	}

	/** Say something politely, once (a screen reader hears it; nothing moves on screen). */
	say(text: string) {
		this.polite = '';
		queueMicrotask(() => (this.polite = text));
	}

	/** A short message under the fingers, read once. Errors stay until dismissed. */
	toast(text: string, warn = false) {
		// the same words again replace the toast already saying them (a failure repeated by a
		// poll or a search does not stack up errors that never leave)
		for (const t of this.toasts) if (t.text === text && t.warn === warn) this.dismiss(t.id);
		const id = this.#next++;
		this.toasts.push({ id, text, warn });
		if (warn) {
			this.assertive = '';
			queueMicrotask(() => (this.assertive = text));
		} else {
			this.say(text);
			this.resume(id);
		}
	}

	/** A gesture failed: felt, said in plain words (errors.ts), and shown until read. */
	fail(error: unknown) {
		haptic(FAILURE);
		this.toast(errorText(error), true);
	}

	dismiss(id: number) {
		clearTimeout(this.#timers.get(id));
		this.#timers.delete(id);
		this.toasts = this.toasts.filter((t) => t.id !== id);
	}

	/** Hover or focus holds a toast; leaving lets it go after 5 s (WCAG 2.2.1). */
	pause(id: number) {
		clearTimeout(this.#timers.get(id));
	}

	resume(id: number) {
		if (this.toasts.find((t) => t.id === id)?.warn) return;
		this.#timers.set(
			id,
			setTimeout(() => this.dismiss(id), 5000)
		);
	}
}

export const ui = new Ui();
