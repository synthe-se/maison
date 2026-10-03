// A server value kept fresh: fetched when a view needs it, polled while the page is seen,
// refreshed right after a gesture. One instance per key, shared by every view that reads it
// (two cards on the same TV ask once). Replaces TanStack Query for a dashboard that only reads
// a handful of endpoints.

type Every<T> = number | ((data: T | undefined) => number);

class Live<T> {
	data = $state.raw<T | undefined>(undefined);
	error = $state.raw<Error | undefined>(undefined);
	/** Only the first load: a refresh keeps showing what is known. */
	loading = $derived(this.data === undefined && this.error === undefined);
	/** Nothing known and the last ask failed: an error to show (with a retry), never an empty
	 * list (« Aucun volet » would be a lie). */
	failed = $derived(this.data === undefined && this.error !== undefined);
	/** When the last answer arrived (ms), to say how old a value is. */
	at = $state(0);

	#fetch: () => Promise<T>;
	#every: Every<T> | undefined;
	#timer: ReturnType<typeof setTimeout> | undefined;
	#inflight: Promise<void> | undefined;
	users = 0;

	constructor(fetch: () => Promise<T>, every?: Every<T>) {
		this.#fetch = fetch;
		this.#every = every;
	}

	/** Ask now (one request at a time: a second ask joins the first). */
	refresh(): Promise<void> {
		this.#inflight ??= this.#fetch()
			.then((data) => {
				this.data = data;
				this.error = undefined;
				this.at = Date.now();
			})
			.catch((e: unknown) => {
				this.error = e instanceof Error ? e : new Error(String(e));
			})
			.finally(() => {
				this.#inflight = undefined;
				this.#schedule();
			});
		return this.#inflight;
	}

	/** What a command answered: shown at once, without waiting for the next poll. The next
	 * poll is planned from it (a motor that starts polls at 1 s, a pairing window that opens
	 * counts down), unless a request is in flight: its answer plans it. */
	set(data: T) {
		this.data = data;
		this.error = undefined;
		this.at = Date.now();
		if (!this.#inflight) this.#schedule();
	}

	start() {
		if (this.users++ === 0) void this.refresh();
	}

	stop() {
		if (--this.users === 0) clearTimeout(this.#timer);
	}

	#schedule() {
		clearTimeout(this.#timer);
		if (!this.users || this.#every === undefined) return;
		const ms = typeof this.#every === 'function' ? this.#every(this.data) : this.#every;
		// a hidden tab does not poll; it asks again when seen (below)
		if (ms > 0 && document.visibilityState === 'visible') this.#timer = setTimeout(() => void this.refresh(), ms);
	}
}

export type { Live };

const all = new Map<string, Live<unknown>>();

/**
 * The value at `key`, kept fresh while the calling component is mounted. `every` is the poll
 * interval in ms (or a function of the current value: faster while a motor runs); omit it for
 * a value that only changes when someone acts. Call it during component initialisation.
 */
export function live<T>(key: string, fetch: () => Promise<T>, every?: Every<T>): Live<T> {
	let entry = all.get(key) as Live<T> | undefined;
	if (!entry) {
		entry = new Live(fetch, every);
		all.set(key, entry as Live<unknown>);
	}
	const it = entry;
	$effect(() => {
		it.start();
		return () => it.stop();
	});
	return it;
}

/** Ask again every value whose key starts with `prefix` (after a gesture changed them). */
export function refresh(prefix: string): Promise<void[]> {
	return Promise.all([...all].filter(([k, v]) => k.startsWith(prefix) && v.users > 0).map(([, v]) => v.refresh()));
}

/** Forget everything (sign-out: the next person starts clean). */
export function forgetAll() {
	all.clear();
}

if (typeof document !== 'undefined') {
	// seen again: everything on screen is asked at once, then polls resume
	document.addEventListener('visibilitychange', () => {
		if (document.visibilityState === 'visible') for (const v of all.values()) if (v.users > 0) void v.refresh();
	});
}
