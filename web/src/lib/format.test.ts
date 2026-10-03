import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { formatDuration, formatMinutes, unreachableSince } from './format.ts';
import { when } from '#lib/i18n.svelte.ts';

describe('formatDuration', () => {
	it('says seconds under a minute, rounded, never negative', () => {
		expect(formatDuration(45)).toBe(m.duration_seconds({ s: 45 }));
		expect(formatDuration(12.6)).toBe(m.duration_seconds({ s: 13 }));
		expect(formatDuration(-5)).toBe(m.duration_seconds({ s: 0 }));
	});

	it('says minutes, with the seconds only when there are some', () => {
		expect(formatDuration(180)).toBe(m.duration_minutes({ m: 3 }));
		expect(formatDuration(200)).toBe(m.duration_minutes_seconds({ m: 3, s: 20 }));
	});

	it('says hours, with the minutes padded to two digits', () => {
		expect(formatDuration(7200)).toBe(m.duration_hours({ h: 2 }));
		expect(formatDuration(3600 + 5 * 60)).toBe(m.duration_hours_minutes({ h: 1, m: 5, mm: '05' }));
		expect(formatDuration(3600 + 5 * 60)).toMatch(/^1\sh\s05$/);
	});

	it('says days, with the hours only when there are some; two units at most', () => {
		expect(formatDuration(86_400)).toBe(m.duration_days({ d: 1 }));
		expect(formatDuration(2 * 86_400 + 4 * 3600 + 59)).toBe(m.duration_days_hours({ d: 2, h: 4 }));
	});
});

describe('formatMinutes', () => {
	it('reads a counter in minutes', () => {
		expect(formatMinutes(90)).toBe(formatDuration(5400));
		expect(formatMinutes(0.5)).toBe(m.duration_seconds({ s: 30 }));
	});
});

describe('unreachableSince', () => {
	it('within the hour: « depuis 12 min »; then the time it was last heard', () => {
		const now = Date.now();
		expect(unreachableSince(now - 12 * 60_000)).toBe(m.state_unreachable_for({ duration: m.duration_minutes({ m: 12 }) }));
		expect(unreachableSince(new Date(now - 30_000).toISOString())).toBe(
			m.state_unreachable_for({ duration: m.duration_minutes({ m: 1 }) })
		);
		const old = now - 3 * 3_600_000;
		expect(unreachableSince(old)).toBe(m.state_unreachable_since({ when: when(old) }));
	});

	it('never heard: says so', () => {
		expect(unreachableSince(0)).toBe(m.state_never_seen());
		expect(unreachableSince(null)).toBe(m.state_never_seen());
	});
});
