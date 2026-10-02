// The Tempo endpoints, each read under one key and at one pace, whoever asks (the dashboard
// tile and the Tempo page share them). Call during component initialisation, like live().

import { tempoApi } from '#lib/api.ts';
import { live } from '#lib/live.svelte.ts';

/** RTE publishes once a day (tomorrow at 10:30): half an hour is plenty, as before. */
const COLOURS_EVERY = 30 * 60_000;
/** The forecasts and the calendar move at the same pace as the colours. */
const FORECAST_EVERY = 30 * 60_000;
/** The season's stock only changes when a day is published. */
const STATE_EVERY = 60 * 60_000;

export const tempoToday = () => live('tempo', tempoApi.get, COLOURS_EVERY);
export const tempoForecasts = () => live('tempo-predictions', tempoApi.getPredictions, FORECAST_EVERY);
export const tempoState = () => live('tempo-state', tempoApi.getState, STATE_EVERY);
export const tempoCalendar = (season: string) => live(`tempo-calendar-${season}`, () => tempoApi.getCalendar(season), FORECAST_EVERY);
