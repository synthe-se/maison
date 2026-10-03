// A server value kept fresh: fetched when a view needs it, polled while the page is seen,
// refreshed right after a gesture. One instance per key, shared by every view that reads it
// (two cards on the same TV ask once). Replaces TanStack Query for a dashboard that only reads
// a handful of endpoints.
//
// What a value is (its key, its one fetch, its pace) is declared once, next to the family's API
// (`devices/<family>/data.ts`): every view of a key asks the same way. Two views declaring the
// same key with different fetches would share whichever came first (a tile's closure reused by
// a page): in development, live() refuses it.

import { time } from '#lib/clock.svelte.ts';

/** The poll interval in ms, or a function of the current value and of when this ask began
 * (faster while a motor runs, stopped once a search has gone on long enough); 0: no poll. */
export type Every<T> = number | ((data: T | undefined, since: number) => number);

/** A live value's declaration. `fetch` may take an argument for an explicit ask (`refresh(arg)`,
 * a forced scan); polls call it without one. */
export interface Source<T, A = never> {
	key: string;
	fetch: (arg?: A) => Promise<T>;
	every?: Every<T>;
}

/** A value read late this many times its cadence is said to be old. */
export const STALE_CADENCES = 3;

class Live<T, A = never> {
	data = $state.raw<T | undefined>(undefined);
	error = $state.raw<Error | undefined>(undefined);
	/** When the last answer arrived (ms): « Dernière lecture 14:20 ». */
	at = $state(0);
	/** When this ask began: the first view of it, or an explicit `refresh(arg)` (a new search). */
	since = $state(0);
	/** A request is travelling. */
	fetching = $state(false);
	/** Only the first load: a refresh keeps showing what is known. */
	loading = $derived(this.data === undefined && this.error === undefined);
	/** Nothing known and the last ask failed: an error to show (with a retry), never an empty
	 * list (« Aucun volet » would be a lie). */
	failed = $derived(this.data === undefined && this.error !== undefined);
	/** Something is shown, but it is old: the last ask failed, or the polls have not answered for
	 * STALE_CADENCES of their interval (docs/ux.md § 4: the last value stays, said as such). */
	stale = $derived.by(() => {
		if (this.data === undefined) return false;
		if (this.error !== undefined) return true;
		const every = this.#cadence();
		return every > 0 && !this.fetching && time.now.getTime() - this.at > STALE_CADENCES * every;
	});

	readonly source: Source<T, A>;
	/** The first answer (now, if one is known): what a form fills itself from, once. */
	readonly ready: Promise<T>;
	#answered: (data: T) => void = () => {};
	#timer: ReturnType<typeof setTimeout> | undefined;
	#inflight: Promise<void> | undefined;
	users = 0;

	constructor(src: Source<T, A>) {
		this.source = src;
		this.ready = new Promise((resolve) => (this.#answered = resolve));
	}

	#cadence(): number {
		const every = this.source.every;
		return typeof every === 'function' ? every(this.data, this.since) : (every ?? 0);
	}

	/** Ask now (one request at a time: a second ask joins the first). With `arg`, a new ask of
	 * its own (a forced scan): it waits for the one in flight, then goes, and starts `since`. */
	refresh(arg?: A): Promise<void> {
		if (arg !== undefined) {
			this.since = Date.now();
			const after = this.#inflight ?? Promise.resolve();
			return after.then(() => this.#ask(arg));
		}
		return (this.#inflight ??= this.#ask());
	}

	#ask(arg?: A): Promise<void> {
		this.fetching = true;
		const asked = this.source
			.fetch(arg)
			.then((data) => this.#take(data))
			.catch((e: unknown) => {
				this.error = e instanceof Error ? e : new Error(String(e));
			})
			.finally(() => {
				if (this.#inflight === asked) this.#inflight = undefined;
				this.fetching = false;
				this.#schedule();
			});
		this.#inflight = asked;
		return asked;
	}

	/** What a command answered: shown at once, without waiting for the next poll. The next
	 * poll is planned from it (a motor that starts polls at 1 s, a pairing window that opens
	 * counts down), unless a request is in flight: its answer plans it. */
	set(data: T) {
		this.#take(data);
		if (!this.#inflight) this.#schedule();
	}

	#take(data: T) {
		this.data = data;
		this.error = undefined;
		this.at = Date.now();
		this.#answered(data);
	}

	/** Part of what is known changed (a command's answer names one shutter, the TV's volume):
	 * `fn` makes the new value from the current one. Nothing known yet: nothing to change. */
	update(fn: (data: T) => T) {
		if (this.data !== undefined) this.set(fn(this.data));
	}

	start() {
		if (this.users++ > 0) return;
		this.since = Date.now();
		void this.refresh();
	}

	stop() {
		if (--this.users === 0) clearTimeout(this.#timer);
	}

	#schedule() {
		clearTimeout(this.#timer);
		if (!this.users) return;
		const ms = this.#cadence();
		// a hidden tab does not poll; it asks again when seen (below)
		if (ms > 0 && document.visibilityState === 'visible') this.#timer = setTimeout(() => void this.refresh(), ms);
	}
}

export type { Live };

/** What a view needs to say a value is loading, failed, old, or there: a `Live`, or several
 * read as one (a group fed by two lists). */
export interface Loadable {
	readonly loading: boolean;
	readonly failed: boolean;
	readonly stale: boolean;
	/** When the last answer arrived (ms). */
	readonly at: number;
	readonly error: Error | undefined;
	refresh(): Promise<unknown>;
}

const all = new Map<string, Live<unknown, unknown>>();

/** A source, declared once (the identity of its `fetch` is what live() checks). */
export function source<T, A = never>(key: string, fetch: (arg?: A) => Promise<T>, every?: Every<T>): Source<T, A> {
	return { key, fetch, every };
}

/**
 * A family of sources, one per argument (a plug's electricity, a lamp's detail): `make` builds
 * one; the same key always gives back the first one built, so every view asks with one fetch.
 */
export function sources<Args extends unknown[], T, A = never>(make: (...args: Args) => Source<T, A>): (...args: Args) => Source<T, A> {
	const made = new Map<string, Source<T, A>>();
	return (...args) => {
		const s = make(...args);
		const known = made.get(s.key);
		if (known) return known;
		made.set(s.key, s);
		return s;
	};
}

/**
 * The value of `src`, kept fresh while the calling component is mounted. Call it during
 * component initialisation.
 */
export function live<T, A = never>(src: Source<T, A>): Live<T, A> {
	let entry = all.get(src.key) as Live<T, A> | undefined;
	if (!entry) {
		entry = new Live(src);
		all.set(src.key, entry as Live<unknown, unknown>);
	} else if (import.meta.env.DEV && entry.source.fetch !== src.fetch) {
		throw new Error(`live('${src.key}'): declared twice with different fetches (declare it once, in the family's data.ts)`);
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
