// A command travelling to a device whose state comes back by reading it
// (docs/ux/tableau-de-bord.md § 2). The gesture is optimistic, the displayed state is not:
//   0 s      the button shows the target at once (`target`), the state line is unchanged
//   > 1 s    `slow`: a ring around the icon and « Allumage… »
//   answer   the device's state wins again (the caller refreshes or sets it)
//   limit    `late`: back to the read state, « Pas de réponse · Réessayer » on the tile
// The limit is per family, set from the measured latency.

import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { FAILURE, haptic } from '#lib/haptics.ts';

export const LIMIT = { lamp: 3_000, plug: 5_000, tv: 30_000, device: 10_000 } as const;

export class Command {
	/** The state asked for, while the command travels (undefined: nothing in flight). */
	target = $state<boolean | undefined>(undefined);
	slow = $state(false);
	/** No answer within the limit, or an error: shown on the tile until the next try. */
	late = $state(false);
	#limit: number;
	#name: () => string;
	#gen = 0;

	/** `name`: the device's name, for the one sentence a screen reader hears on failure. */
	constructor(name: () => string, limit: number = LIMIT.device) {
		this.#name = name;
		this.#limit = limit;
	}

	/** What the button shows: the target while in flight, otherwise what the device said. */
	shown(read: boolean): boolean {
		return this.target ?? read;
	}

	async run(target: boolean, send: () => Promise<unknown>): Promise<boolean> {
		const gen = ++this.#gen;
		this.target = target;
		this.slow = false;
		this.late = false;
		const slow = setTimeout(() => gen === this.#gen && (this.slow = true), 1_000);
		let limitTimer: ReturnType<typeof setTimeout> | undefined;
		const limit = new Promise<'late'>((r) => (limitTimer = setTimeout(() => r('late'), this.#limit)));
		try {
			const outcome = await Promise.race([send().then(() => 'done' as const), limit]);
			if (gen !== this.#gen) return false; // a newer gesture took over
			if (outcome === 'late') this.#fail();
			return outcome === 'done';
		} catch (e) {
			if (gen === this.#gen) this.#fail(e);
			return false;
		} finally {
			clearTimeout(slow);
			clearTimeout(limitTimer);
			if (gen === this.#gen) {
				this.target = undefined;
				this.slow = false;
			}
		}
	}

	#fail(e?: unknown) {
		this.late = true;
		haptic(FAILURE);
		ui.say(m.command_no_answer({ name: this.#name() }));
		if (e) console.warn(e);
	}
}
