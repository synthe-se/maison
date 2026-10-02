// Seasons as Tempo counts them: September to August. Days and numbers come from
// #lib/i18n.svelte.ts.

/** The Tempo season a month belongs to: September starts it (« 2026-2027 »). `month` is 0-11. */
export function seasonOf(year: number, month: number): string {
	return month >= 8 ? `${year}-${year + 1}` : `${year - 1}-${year}`;
}
