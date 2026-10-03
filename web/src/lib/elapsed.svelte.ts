// How long a long step has been going (docs/ux.md § 2 and § 6: waking the TV from deep standby,
// pairing a shutter), said on screen and drawn by Progress: counted from the moment `running`
// turns true, ticking while it is, back to 0 once it stops. Construct during component
// initialisation (it owns an effect).

export class Elapsed {
	#since = $state(0);
	#now = $state(0);

	/** `running`: whether the step is going; `every`: how often the count moves (ms). */
	constructor(running: () => boolean, every = 1_000) {
		$effect(() => {
			if (!running()) {
				this.#since = 0;
				return;
			}
			this.#since = this.#now = Date.now();
			const timer = setInterval(() => (this.#now = Date.now()), every);
			return () => clearInterval(timer);
		});
	}

	/** Milliseconds since the step began; 0 when none runs. */
	get ms(): number {
		return this.#since ? Math.max(0, this.#now - this.#since) : 0;
	}

	/** Whole seconds, as said (« 12 s »). */
	get seconds(): number {
		return Math.round(this.ms / 1000);
	}
}
