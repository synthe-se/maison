// Tempo's API: RTE's colors, the prices, Maison's forecast (backend/src/tempo/).

import { get, path } from '#lib/api.ts';
export type TempoColor = 'BLUE' | 'WHITE' | 'RED';
export type TempoProbabilities = Record<TempoColor, number>;

/** €/kWh, tax included: off-peak and peak. */
export interface TempoPrice {
	offPeak: number;
	peak: number;
}

export interface TempoTariffs {
	blue: TempoPrice;
	white: TempoPrice;
	red: TempoPrice;
	/** The yearly subscription (6 kVA), €. */
	subscription: number | null;
	startsOn: string;
}

export interface TempoCount {
	used: number;
	total: number;
	remaining: number;
}

/** A season's days per color: published (tomorrow included), quota, left. */
export interface TempoStock {
	season: string;
	blue: TempoCount;
	white: TempoCount;
	red: TempoCount;
}

export interface TempoDay {
	date: string;
	/** Null until RTE publishes it. */
	color: TempoColor | null;
}

/** Peak hours (« HH:MM », local): off-peak the rest of the day. */
export interface TempoHours {
	peakStart: string;
	peakEnd: string;
}

export interface TempoToday {
	success: boolean;
	/** In force until 06:00 (a Tempo day runs 06:00 to 06:00). */
	yesterday: TempoDay;
	today: TempoDay;
	tomorrow: TempoDay;
	tariffs: TempoTariffs | null;
	hours: TempoHours;
	stock: TempoStock;
	lastUpdated: string | null;
	/** The sources did not answer: what is shown is older. */
	cached: boolean;
}

export interface TempoScore {
	horizon: number;
	days: number;
	accuracy: number;
	winterAccuracy: number;
	redF1: number;
	whiteF1: number;
	brier: number;
	alwaysBlue: number;
}

export interface TempoForecastDay {
	date: string;
	/** Days after today. */
	horizon: number;
	/** RTE's color, not a forecast. */
	official: boolean;
	color: TempoColor;
	probabilities: TempoProbabilities;
	confidence: number;
	reliability?: { accuracy: number; winterAccuracy: number };
}

export interface TempoForecast {
	success: boolean;
	issued: string;
	/** The weather forecast's day; older than today when Open-Meteo did not answer. */
	weatherIssued: string | null;
	stale: boolean;
	model: { version: string; fittedThrough: string; backtest: { seasons: string[]; horizons: TempoScore[] } } | null;
	days: TempoForecastDay[];
	stock: TempoStock;
	note?: string;
}

export interface TempoCalendarDay {
	date: string;
	color: TempoColor;
	isActual: boolean;
	isPrediction: boolean;
	probabilities?: TempoProbabilities;
	confidence?: number;
}

export interface TempoCalendar {
	success: boolean;
	season: string;
	calendar: TempoCalendarDay[];
	stock: TempoStock;
}

export const tempoApi = {
	get: () => get<TempoToday>('/tempo'),
	forecast: () => get<TempoForecast>('/tempo/forecast'),
	calendar: (season?: string) => get<TempoCalendar>(season ? path`/tempo/calendar?season=${season}` : '/tempo/calendar')
};
