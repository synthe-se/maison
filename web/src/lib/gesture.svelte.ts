// A gesture's whole life, said once: what is in flight (so its own control shows it and does
// not fire twice), the outcome shown or said, a failure felt, said and shown until read
// (docs/ux.md § 2 and § 4). Every button that sends something goes through
// a Gesture; a device tile's on/off goes through Command (command.svelte.ts), which adds the
// optimistic target and the « no answer » limit.
//
// A control whose gesture travels keeps the focus: it is never `disabled` (the focus would
// fall to the page, WCAG 2.4.3), it says it is busy (`pending`), and pressing it again does
// nothing (`run` ignores a key already in flight). A control that cannot act at all (its
// device out of reach) says why (`unavailable`).

import { tick } from 'svelte';
import { ui } from '#lib/ui.svelte.ts';
import { errorText } from '#lib/errors.ts';
import { FAILURE, haptic } from '#lib/haptics.ts';

/** The attributes of a control whose gesture is in flight, to spread on it: busy, not
 * operable, still focusable. Nothing when idle (so another spread is never undone). */
export function pending(busy: boolean): { 'aria-disabled'?: 'true'; 'aria-busy'?: 'true' } {
	return busy ? { 'aria-disabled': 'true', 'aria-busy': 'true' } : {};
}

/** The attributes of a control that cannot act, to spread on it: not operable, still in place
 * and focusable, described by the element (id `reason`) that says why — the tile's state line
 * (« Injoignable depuis 3 min »), an « offline » line. Nothing when it can act (`reason`
 * false). Its handler still checks: aria-disabled does not stop a click. */
export function unavailable(reason: string | false | null | undefined): { 'aria-disabled'?: 'true'; 'aria-describedby'?: string } {
	return reason ? { 'aria-disabled': 'true', 'aria-describedby': reason } : {};
}

export interface RunOptions {
	/** The field the failure is about: the error is shown under it (`error`), not in a toast,
	 * and the focus goes back to it so its description is read (WCAG 3.3.1). */
	field?: () => HTMLElement | null | undefined;
	/** The failure is said where the gesture is (`error`, under its button), not in a toast; the
	 * focus stays where it is (a sign-in, a search). */
	inline?: boolean;
	/** A refusal the caller answers itself (a name already someone's: offer their access
	 * back): true when it did, and nothing else is said. */
	refused?: (e: unknown) => boolean | void;
}

export class Gesture {
	/** What is in flight: the key given to `run` (one gesture at a time per owner). */
	busy = $state<string | null>(null);
	/** The last failure said in place (`RunOptions.field` or `inline`), in words; '' when none. */
	error = $state('');

	/** Whether `key` (or, without one, anything) is in flight. */
	is(key?: string): boolean {
		return key === undefined ? this.busy !== null : this.busy === key;
	}

	/**
	 * Sends, then hands the answer to `then` (show it, read the device again, say it). Returns
	 * the answer, or undefined when it failed (the failure is already told) or when the same
	 * key was already in flight (a second press of a busy control).
	 */
	async run<T>(send: () => Promise<T>, then?: (answer: T) => unknown, key = '', options: RunOptions = {}): Promise<T | undefined> {
		if (this.busy === key) return undefined;
		this.busy = key;
		this.error = '';
		try {
			const answer = await send();
			await then?.(answer);
			return answer;
		} catch (e) {
			if (options.refused?.(e)) return undefined;
			if (options.field || options.inline) {
				haptic(FAILURE);
				this.error = errorText(e);
				if (options.field) {
					await tick();
					options.field()?.focus();
				}
			} else ui.fail(e);
			return undefined;
		} finally {
			if (this.busy === key) this.busy = null;
		}
	}
}
