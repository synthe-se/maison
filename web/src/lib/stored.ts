// What the app remembers in this browser (localStorage), the one way: read with a fallback,
// written or forgotten, and never a failure — private mode or a blocked storage keeps the
// choice for the visit. The keys come from src/env.ts (`$app/env/public`).

export interface Stored<T> {
	get(): T;
	/** `undefined` forgets it (the fallback comes back). */
	set(value: T | undefined): void;
}

/** How a value is written: JSON by default; `text` for a word app.html reads before any
 * script (the theme). `read` gives undefined for what it does not accept. */
export interface Codec<T> {
	read(raw: string): T | undefined;
	write(value: T): string;
}

export const json = <T>(accept: (v: unknown) => v is T): Codec<T> => ({
	read: (raw) => {
		const v: unknown = JSON.parse(raw);
		return accept(v) ? v : undefined;
	},
	write: (v) => JSON.stringify(v)
});

/** One of `words`, kept as itself. */
export const text = <T extends string>(words: readonly T[]): Codec<T> => ({
	read: (raw) => ((words as readonly string[]).includes(raw) ? (raw as T) : undefined),
	write: (v) => v
});

/** The value kept under `key`, or `fallback` (nothing kept, unreadable, or not accepted). */
export function stored<T>(key: string, fallback: T, codec: Codec<T>): Stored<T> {
	return {
		get() {
			try {
				const raw = localStorage.getItem(key);
				return (raw === null ? undefined : codec.read(raw)) ?? fallback;
			} catch {
				return fallback;
			}
		},
		set(value) {
			try {
				if (value === undefined) localStorage.removeItem(key);
				else localStorage.setItem(key, codec.write(value));
			} catch {
				// private mode: remembered for the visit
			}
		}
	};
}
