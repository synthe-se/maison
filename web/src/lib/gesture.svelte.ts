// A gesture's whole life, said once: what is in flight (so its own control shows it and does
// not fire twice), the outcome shown or said, a failure felt, said and shown until read
// (docs/ux/tableau-de-bord.md § 2 and § 4). Every button that sends something goes through
// a Gesture; a device tile's on/off goes through Command (command.svelte.ts), which adds the
// optimistic target and the « no answer » limit.
//
// A control whose gesture travels keeps the focus: it is never `disabled` (the focus would
// fall to the page, WCAG 2.4.3), it says it is busy (`pending`), and pressing it again does
// nothing (`run` ignores a key already in flight).

import { tick } from 'svelte';
import { ui } from '#lib/ui.svelte.ts';
import { errorText } from '#lib/errors.ts';
import { FAILURE, haptic } from '#lib/haptics.ts';

/** The attributes of a control whose gesture is in flight, to spread on it: busy, not
 * operable, still focusable. */
export function pending(busy: boolean) {
	return { 'aria-disabled': busy ? ('true' as const) : undefined, 'aria-busy': busy ? ('true' as const) : undefined };
}

export interface RunOptions {
	/** The field the failure is about: the error is shown under it (`error`), not in a toast,
	 * and the focus goes back to it so its description is read (WCAG 3.3.1). */
	field?: () => HTMLElement | null | undefined;
}

export class Gesture {
	/** What is in flight: the key given to `run` (one gesture at a time per owner). */
	busy = $state<string | null>(null);
	/** The last failure tied to a field (`RunOptions.field`), in words; '' when none. */
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
			if (options.field) {
				haptic(FAILURE);
				this.error = errorText(e);
				await tick();
				options.field()?.focus();
			} else ui.fail(e);
			return undefined;
		} finally {
			if (this.busy === key) this.busy = null;
		}
	}
}
