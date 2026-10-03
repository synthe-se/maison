// Where the focus goes when the focused control disappears (a row removed, a form closed, a
// view swapped): never left to fall on the page (WCAG 2.4.3). The first candidate still on the
// page takes it, after the DOM has caught up; a heading is made focusable for it (tabindex -1,
// no outline: app.css), as the remote page does with its list title.

import { tick } from 'svelte';

/** An element, a selector, or a getter (for an element the change itself brings). */
type Target = HTMLElement | string | null | undefined | (() => HTMLElement | null | undefined);

/** Focuses the first of `targets` (elements, or selectors) that is on the page. */
export async function refocus(...targets: Target[]): Promise<HTMLElement | null> {
	await tick();
	for (const t of targets) {
		const el = typeof t === 'string' ? document.querySelector<HTMLElement>(t) : typeof t === 'function' ? t() : t;
		if (!el?.isConnected) continue;
		if (el.tabIndex < 0 && !el.hasAttribute('tabindex')) el.tabIndex = -1;
		el.focus();
		return el;
	}
	return null;
}

/** The title of the section around `el` (its first heading), for a removed row's focus. */
export function sectionHeading(el: Element | null | undefined): HTMLElement | null {
	return el?.closest('section, article, [role="dialog"]')?.querySelector<HTMLElement>('h1, h2, h3, h4') ?? null;
}
