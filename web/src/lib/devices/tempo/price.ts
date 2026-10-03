// What electricity costs, once for the whole app: the price in force at a moment (the color
// of the Tempo day, 06:00 to 06:00, so before 06:00 it is yesterday's; and the period, peak
// from 06:00 to 22:00, off-peak the rest), what a device drawing some watts costs per hour,
// and a month's estimate from daily totals. Worked out here from what /tempo sent, on a
// timer: the boundaries need no request.

import { m } from '#lib/paraglide/messages.js';
import { lower, num } from '#lib/i18n.svelte.ts';
import type { TempoColor, TempoHours, TempoTariffs, TempoToday } from '#lib/devices/tempo/api.ts';
import { TEMPO } from './colors.ts';

export type Period = 'peak' | 'offPeak';

export interface PriceNow {
	color: TempoColor;
	period: Period;
	/** €/kWh */
	price: number;
	/** When the period ends (« 22:00 »). */
	until: string;
	/** The same color's other period. */
	other: { period: Period; price: number };
	/** False before the peak hours start: the color in force is still yesterday's, and
	 * today's (`next`) starts at `until`. */
	dayStarted: boolean;
	next: TempoColor | null;
}

const minutes = (hhmm: string) => Number(hhmm.slice(0, 2)) * 60 + Number(hhmm.slice(3, 5));

/** The period at `now`; the Tempo day starts when the peak hours do. */
function periodAt(now: Date, hours: TempoHours): { period: Period; dayStarted: boolean } {
	const t = now.getHours() * 60 + now.getMinutes();
	const [start, end] = [minutes(hours.peakStart), minutes(hours.peakEnd)];
	return { period: t >= start && t < end ? 'peak' : 'offPeak', dayStarted: t >= start };
}

/** What a kWh costs at `now`; `null` without prices or without the color in force. */
export function priceNow(now: Date, data: TempoToday): PriceNow | null {
	const { period, dayStarted } = periodAt(now, data.hours);
	const color = dayStarted ? data.today.color : data.yesterday.color;
	if (!color || !data.tariffs) return null;
	const prices = data.tariffs[TEMPO[color].key];
	const other: Period = period === 'peak' ? 'offPeak' : 'peak';
	return {
		color,
		period,
		price: prices[period],
		until: period === 'peak' ? data.hours.peakEnd : data.hours.peakStart,
		other: { period: other, price: prices[other] },
		dayStarted,
		next: dayStarted ? null : data.today.color
	};
}

/** The part of a day spent in peak hours (16 h of 24 with 06:00–22:00). */
export function peakShare(hours: TempoHours): number {
	return (minutes(hours.peakEnd) - minutes(hours.peakStart)) / (24 * 60);
}

/** €/h for a device drawing `watts` at `eurPerKwh`. */
export const costPerHour = (watts: number, eurPerKwh: number) => (watts / 1000) * eurPerKwh;

/** Under this, a cost per hour reads better in cents (« 0,7 c€/h »). */
const CENTS_BELOW = 0.1;

/** « 0,7 c€/h », « 0,76 €/h ». */
export function perHour(eurPerHour: number): string {
	return eurPerHour < CENTS_BELOW
		? m.price_cents_per_hour({ cents: num(eurPerHour * 100, 1) })
		: m.price_euros_per_hour({ euros: num(eurPerHour, 2) });
}

/** « 4,12 € » */
export const euros = (e: number) => m.price_euros({ euros: num(e, 2) });

/** « HP bleu »: the period and the color in a phrase. */
export const periodColor = (period: Period, color: TempoColor) =>
	m.tempo_period_color({ period: period === 'peak' ? m.tempo_hp() : m.tempo_hc(), color: lower(TEMPO[color].name()) });

/** Peak hours in force on a red day: the costliest kWh of the year. */
export const redPeak = (p: PriceNow | null) => p?.color === 'RED' && p.period === 'peak';

export interface MonthEstimate {
	/** € for the days counted. */
	euros: number;
	/** Days of the month counted (a reading and a color). */
	days: number;
}

/**
 * What the month so far cost: each day's Wh at that day's color, split between peak and
 * off-peak hours in proportion to their length (a device drawing evenly, the stated
 * assumption: 2/3 peak with 06:00–22:00). A day without a known color is left out. `month`:
 * « 2026-10 ».
 */
export function monthEstimate(
	days: { date: string; wh: number }[],
	colorOf: (iso: string) => TempoColor | null | undefined,
	tariffs: TempoTariffs,
	hours: TempoHours,
	month: string
): MonthEstimate {
	const share = peakShare(hours);
	let total = 0;
	let counted = 0;
	for (const d of days) {
		if (!d.date.startsWith(month)) continue;
		const color = colorOf(d.date);
		if (!color) continue;
		const p = tariffs[TEMPO[color].key];
		total += (d.wh / 1000) * (share * p.peak + (1 - share) * p.offPeak);
		counted++;
	}
	return { euros: total, days: counted };
}
