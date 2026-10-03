// The Tempo endpoints, each read under one key and at one pace, whoever asks (the dashboard
// tile and the Tempo page share them). Call during component initialisation, like live().

import { tempoApi } from '#lib/api.ts';
import { live } from '#lib/live.svelte.ts';

/** RTE publishes once a day (tomorrow around 10:40); the server asks it again every quarter
 * of an hour until then: half an hour here is plenty. */
const EVERY = 30 * 60_000;

export const tempoToday = () => live('tempo', tempoApi.get, EVERY);
export const tempoForecast = () => live('tempo-forecast', tempoApi.forecast, EVERY);
export const tempoCalendar = (season: string) => live(`tempo-calendar-${season}`, () => tempoApi.calendar(season), EVERY);
