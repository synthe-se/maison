// A « Maintenant » chip leads to its group: the group's title scrolled to the top and focused
// (WCAG 2.4.3: the reader goes on from there), never a reordered dashboard.

import { refocus } from '#lib/focus.ts';

export function jump(titleId: string) {
	const el = document.getElementById(titleId);
	if (!el) return;
	el.scrollIntoView({ block: 'start' });
	void refocus(el);
}
