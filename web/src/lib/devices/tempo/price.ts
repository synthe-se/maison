// The price in force at a moment: the colour of the Tempo day (06:00 to 06:00, so before
// 06:00 it is yesterday's) and the period (peak from 06:00 to 22:00, off-peak the rest), from
// what /tempo sent. Worked out here, on a timer: the boundaries need no request.

import type { TempoColor, TempoHours, TempoToday } from '#lib/api.ts';
import { TEMPO } from './colors.ts';

export type Period = 'hp' | 'hc';

export interface PriceNow {
	color: TempoColor;
	period: Period;
	/** €/kWh */
	price: number;
	/** When the period ends (« 22:00 »). */
	until: string;
	/** The same colour's other period. */
	other: { period: Period; price: number };
}

const minutes = (hhmm: string) => Number(hhmm.slice(0, 2)) * 60 + Number(hhmm.slice(3, 5));

/** The period at `now`; the Tempo day starts when the peak hours do. */
function periodAt(now: Date, hours: TempoHours): { period: Period; dayStarted: boolean } {
	const t = now.getHours() * 60 + now.getMinutes();
	const [start, end] = [minutes(hours.peak_start), minutes(hours.peak_end)];
	return { period: t >= start && t < end ? 'hp' : 'hc', dayStarted: t >= start };
}

/** What a kWh costs at `now`; `null` without prices or without the colour in force. */
export function priceNow(now: Date, data: TempoToday): PriceNow | null {
	const { period, dayStarted } = periodAt(now, data.hours);
	const color = dayStarted ? data.today.color : data.yesterday.color;
	if (!color || !data.tarifs) return null;
	const prices = data.tarifs[TEMPO[color].key];
	const other: Period = period === 'hp' ? 'hc' : 'hp';
	return {
		color,
		period,
		price: prices[period],
		until: period === 'hp' ? data.hours.peak_end : data.hours.peak_start,
		other: { period: other, price: prices[other] }
	};
}
