// Tempo fixtures as the backend sends them (backend/src/tempo/mod.rs), for the Tempo tests.

import type { TempoColor, TempoForecast, TempoForecastDay, TempoStock, TempoToday } from '#lib/api.ts';

export const TARIFS = {
	blue: { hc: 0.1356, hp: 0.1654 },
	white: { hc: 0.1536, hp: 0.1921 },
	red: { hc: 0.1615, hp: 0.7295 },
	subscription: 189.98,
	dateDebut: '2026-08-01'
};

export function tempoStock(over: Partial<TempoStock> = {}): TempoStock {
	return {
		season: '2026-2027',
		blue: { used: 95, total: 300, remaining: 205 },
		white: { used: 3, total: 43, remaining: 40 },
		red: { used: 4, total: 22, remaining: 18 },
		...over
	};
}

export function tempoToday(over: Partial<TempoToday> = {}): TempoToday {
	return {
		success: true,
		yesterday: { date: '2026-12-09', color: 'WHITE' },
		today: { date: '2026-12-10', color: 'RED' },
		tomorrow: { date: '2026-12-11', color: null },
		tarifs: TARIFS,
		hours: { peak_start: '06:00', peak_end: '22:00' },
		stock: tempoStock(),
		lastUpdated: '2026-12-10T09:00:00Z',
		cached: false,
		...over
	};
}

/** A forecast day: `color` at `confidence`, the rest shared by the two other colours. */
export function forecastDay(date: string, horizon: number, color: TempoColor, confidence: number, official = false): TempoForecastDay {
	const rest = (1 - confidence) / 2;
	const probabilities = { BLUE: rest, WHITE: rest, RED: rest, [color]: confidence };
	return {
		date,
		horizon,
		official,
		color,
		probabilities,
		confidence,
		...(official ? {} : { reliability: { accuracy: 0.955 - horizon * 0.01, winter_accuracy: 0.914 - horizon * 0.02 } })
	};
}

const HORIZONS = [1, 2, 3, 4, 5, 6, 7].map((h) => ({
	horizon: h,
	days: 716,
	accuracy: 0.955 - h * 0.01,
	winter_accuracy: 0.914 - h * 0.02,
	red_f1: 0.9,
	white_f1: 0.8,
	brier: 0.07,
	always_blue: 0.818
}));

export function tempoForecast(over: Partial<TempoForecast> = {}): TempoForecast {
	return {
		success: true,
		issued: '2026-12-10',
		weather_issued: '2026-12-10',
		stale: false,
		model: { version: '2026-10-03', fitted_through: '2026-10-01', backtest: { seasons: ['2024-2025', '2025-2026'], horizons: HORIZONS } },
		days: [forecastDay('2026-12-11', 1, 'WHITE', 1, true), forecastDay('2026-12-12', 2, 'RED', 0.62), forecastDay('2026-12-13', 3, 'BLUE', 0.45)],
		stock: tempoStock(),
		...over
	};
}
