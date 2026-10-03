// A group's action on all its devices at once (« Tout éteindre », « Tout fermer »): every
// order sent together, none waiting for another, and the outcome said once: how many did it,
// or which ones did not answer (docs/ux.md § 2). Run inside a Gesture: its control shows the
// wait and keeps the focus.

import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { list } from '#lib/i18n.svelte.ts';
import { FAILURE, haptic } from '#lib/haptics.ts';

export interface Outcome {
	/** The names of the devices that did it. */
	done: string[];
	/** The names of those that refused or did not answer. */
	failed: string[];
}

/** Sends `send` to each item at once; never throws: a failure is one name in `failed`. */
export async function everyone<T>(items: T[], name: (t: T) => string, send: (t: T) => Promise<unknown>): Promise<Outcome> {
	const settled = await Promise.allSettled(items.map(send));
	const names = items.map(name);
	return {
		done: names.filter((_, i) => settled[i].status === 'fulfilled'),
		failed: names.filter((_, i) => settled[i].status === 'rejected')
	};
}

/** Says the outcome: all done in `said(count)`; else a warning that stays, naming who failed. */
export function sayOutcome(o: Outcome, said: (count: number) => string) {
	if (!o.failed.length) return ui.toast(said(o.done.length));
	haptic(FAILURE);
	ui.toast(m.group_partial({ done: said(o.done.length), names: list(o.failed, 'long') }), { warn: true });
}
