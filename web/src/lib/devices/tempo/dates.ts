// Seasons as Tempo counts them: September to August. Days and numbers come from
// #lib/i18n.svelte.ts.

/** The Tempo season a month belongs to: September starts it (« 2026-2027 »). `month` is 0-11. */
export function seasonOf(year: number, month: number): string {
	return month >= 8 ? `${year}-${year + 1}` : `${year - 1}-${year}`;
}

/** Whether `season` (« 2025-2026 ») is over at `now`: its days are all known, they never change. */
export function seasonOver(season: string, now: Date): boolean {
	const end = Number(season.slice(5));
	return now >= new Date(end, 8, 1);
}
