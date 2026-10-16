// The Tempo endpoints, each read under one key and at one pace, whoever asks (the dashboard
// tile, the Tempo page, the plugs' costs share them): `live(today)`.

import { source, sources } from '#lib/live.svelte.ts';
import { TEMPO_MS } from '#lib/paces.ts';
import { tempoApi } from './api.ts';
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
