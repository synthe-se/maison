// The Tempo endpoints, each read under one key and at one pace, whoever asks (the dashboard
// tile, the Tempo page, the plugs' costs share them): `live(today)`.

import { source, sources } from '#lib/live.svelte.ts';
import { tempoApi } from './api.ts';

/** RTE publishes once a day (tomorrow around 10:40); the server asks it again every quarter
 * of an hour until then: half an hour here is plenty. */
const EVERY = 30 * 60_000;

export const today = source('tempo:today', tempoApi.get, EVERY);
export const forecast = source('tempo:forecast', tempoApi.forecast, EVERY);
/** One season's calendar (« 2026-2027 »). */
export const calendar = sources((season: string) => source(`tempo:calendar:${season}`, () => tempoApi.calendar(season), EVERY));
