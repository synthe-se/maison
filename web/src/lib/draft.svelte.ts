// A form's draft, the one way to know it changed (a meal, a remote button, a scene, an
// invitation): what it opened with, what it holds now, and whether they differ. Sheet reads
// `dirty` to ask before a stray Escape loses it (docs/ux.md § 6).

export class Draft<T> {
	current = $state<T>(undefined as T);
	#initial = $state('');
	/** Changed from what it opened with (or was last reset to). */
	dirty = $derived(JSON.stringify(this.current) !== this.#initial);

	constructor(initial: T) {
		this.reset(initial);
	}

	/** Starts again from `value` (a binding adopted, a form saved): not changed any more. */
	reset(value: T) {
		// a copy: editing the draft never touches what it came from
		this.current = $state.snapshot(value) as T;
		this.#initial = JSON.stringify(this.current);
	}
}
