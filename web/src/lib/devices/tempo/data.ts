// The Tempo endpoints, each read under one key and at one pace, whoever asks (the dashboard
// tile, the Tempo page, the plugs' costs share them): `live(today)`.

import { live, source, sources } from '#lib/live.svelte.ts';
import { TEMPO_MS } from '#lib/paces.ts';
import { time } from '#lib/clock.svelte.ts';
import { tempoApi } from './api.ts';
import { priceNow } from './price.ts';
import { seasonOver } from './dates.ts';

export const today = source('tempo:today', tempoApi.get, TEMPO_MS);
export const forecast = source('tempo:forecast', tempoApi.forecast, TEMPO_MS);
/** One season's calendar (« 2026-2027 »): a past season's never changes, it is read once. */
export const calendar = sources((season: string) =>
	source(
		`tempo:calendar:${season}`,
		() => tempoApi.calendar(season),
		() => (seasonOver(season, new Date()) ? 0 : TEMPO_MS)
	)
);

/** The price in force now (the shared clock: it turns at 06:00 and 22:00 by itself), and the
 * day it comes from: what the plugs' costs and « Maintenant » read. Call during component
 * initialisation. */
export function livePrice() {
	const tempo = live(today);
	return {
		tempo,
		get current() {
			return tempo.data ? priceNow(time.now, tempo.data) : null;
		}
	};
}
