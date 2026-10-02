// A gesture's whole life, said once: what is in flight (so its own control shows it and does
// not fire twice), the outcome shown or said, a failure felt, said and shown until read
// (docs/ux/tableau-de-bord.md § 2 and § 4). Every button that sends something goes through
// a Gesture; a device tile's on/off goes through Command (command.svelte.ts), which adds the
// optimistic target and the « no answer » limit.

import { ui } from '#lib/ui.svelte.ts';

export class Gesture {
	/** What is in flight: the key given to `run` (one gesture at a time per owner). */
	busy = $state<string | null>(null);

	/** Whether `key` (or, without one, anything) is in flight. */
	is(key?: string): boolean {
		return key === undefined ? this.busy !== null : this.busy === key;
	}

	/**
	 * Sends, then hands the answer to `then` (show it, read the device again, say it). Returns
	 * the answer, or undefined when it failed (the failure is already told).
	 */
	async run<T>(send: () => Promise<T>, then?: (answer: T) => unknown, key = ''): Promise<T | undefined> {
		this.busy = key;
		try {
			const answer = await send();
			await then?.(answer);
			return answer;
		} catch (e) {
			ui.fail(e);
			return undefined;
		} finally {
			if (this.busy === key) this.busy = null;
		}
	}
}
