// The time of day every view reads, said once: the Tempo price in force, a shutter's next
// move, a plug's cost per hour. Ticks every half minute (the boundaries need no request); a
// test sets `time.now` to the moment it needs.

/** Often enough for a price that changes at 06:00 and 22:00 sharp. */
export const TICK = 30_000;

export const time = $state({ now: new Date() });

if (typeof window !== 'undefined') setInterval(() => (time.now = new Date()), TICK);
