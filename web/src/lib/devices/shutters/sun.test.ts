import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { formatMinutes } from '#lib/format.ts';
import { clock } from '#lib/i18n.svelte.ts';
import { shutter } from '#lib/test/shutters.ts';
import { DEFAULT_OFFSET, moveSentence, nextEvent, nextMove, OFFSET_VALUES, OFFSETS, offsetLabel, skipLabel, toggled } from './sun.ts';

const none = { openAtSunrise: false, closeAtSunset: false, sunriseOffsetMin: 0, sunsetOffsetMin: 0 };

describe('the sun schedule in words', () => {
	it('says an offset before, at, or after the sun', () => {
		expect(offsetLabel(0)).toBe(m.shutters_offset_on_time());
		expect(offsetLabel(60)).toBe(m.shutters_offset_after({ duration: formatMinutes(60) }));
		expect(offsetLabel(-30)).toBe(m.shutters_offset_before({ duration: formatMinutes(30) }));
	});

	it('offers every offset within three hours, in order, the sun’s time included', () => {
		expect(OFFSETS[0]).toBe(-180);
		expect(OFFSETS.at(-1)).toBe(180);
		expect([...OFFSETS].toSorted((a, b) => a - b)).toEqual([...OFFSETS]);
		expect(OFFSET_VALUES).toContain('0');
		expect(offsetLabel(Number('0'))).toBe(m.shutters_offset_on_time());
	});

	it('closes an hour after sunset when the evening event is first turned on', () => {
		expect(DEFAULT_OFFSET.sunset).toBe(60);
		expect(toggled(none, 'sunset', true)).toEqual({ ...none, closeAtSunset: true, sunsetOffsetMin: 60 });
		expect(toggled(none, 'sunrise', true)).toEqual({ ...none, openAtSunrise: true, sunriseOffsetMin: 0 });
	});

	it('keeps a chosen offset when the event is switched off and on again', () => {
		const chosen = { ...none, closeAtSunset: true, sunsetOffsetMin: 90 };
		const off = toggled(chosen, 'sunset', false);
		expect(off).toEqual({ ...chosen, closeAtSunset: false });
		expect(toggled(off, 'sunset', true).sunsetOffsetMin).toBe(60); // turned on anew: the default
		expect(toggled(chosen, 'sunset', true).sunsetOffsetMin).toBe(90); // already on: untouched
	});

	it('the tile says the next move, the sooner of the two', () => {
		const open = '2026-10-03T05:53:00Z';
		const close = '2026-10-02T18:28:00Z';
		expect(nextMove(shutter({ nextOpen: open, nextClose: close }))).toBe(m.shutters_fact_close({ time: clock(new Date(close)) }));
		expect(nextMove(shutter({ nextOpen: open }))).toBe(m.shutters_fact_open({ time: clock(new Date(open)) }));
		expect(nextMove(shutter())).toBeUndefined();
	});

	it('a skipped move is said skipped, and left out of the « Maintenant » sentence', () => {
		const close = '2026-10-02T18:28:00Z';
		const c = shutter({ name: 'Volet salon', nextClose: close, skipNextClose: true });
		expect(nextEvent(c)).toEqual({ event: 'close', at: close, skipped: true });
		expect(nextMove(c)).toBe(m.shutters_close_skipped({ time: clock(new Date(close)) }));
		expect(moveSentence(c)).toBeUndefined();
		expect(moveSentence({ ...c, skipNextClose: false })).toBe(m.shutters_will_close({ name: 'Volet salon', time: clock(new Date(close)) }));
	});

	it('names the skip by its day: tonight, tomorrow morning, else plainly', () => {
		const at = (offset: number, h: number) => {
			const d = new Date();
			d.setDate(d.getDate() + offset);
			d.setHours(h, 0, 0, 0);
			return d.toISOString();
		};
		expect(skipLabel('close', at(0, 20))).toBe(m.shutters_skip_close_tonight());
		expect(skipLabel('close', at(1, 20))).toBe(m.shutters_skip_close_tomorrow());
		expect(skipLabel('close', at(3, 20))).toBe(m.shutters_skip_close());
		expect(skipLabel('open', at(1, 7))).toBe(m.shutters_skip_open_tomorrow());
		expect(skipLabel('open', at(0, 7))).toBe(m.shutters_skip_open_today());
		expect(skipLabel('open', at(4, 7))).toBe(m.shutters_skip_open());
	});
});
