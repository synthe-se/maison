import { afterEach, describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { list } from '#lib/i18n.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { everyone, sayOutcome } from './everyone.ts';

afterEach(() => (ui.toasts = []));

describe('everyone', () => {
	it('sends to each at once and names who did and who did not, never throwing', async () => {
		const started: string[] = [];
		const o = await everyone(
			['A', 'B', 'C'],
			(x) => x,
			async (x) => {
				started.push(x);
				if (x === 'B') throw new Error('timeout');
			}
		);
		expect(started).toEqual(['A', 'B', 'C']);
		expect(o).toEqual({ done: ['A', 'C'], failed: ['B'] });
	});

	it('says all done plainly, a partial failure in a warning that names the silent ones', () => {
		sayOutcome({ done: ['A', 'C'], failed: [] }, (count) => m.lamps_all_off_done({ count }));
		expect(ui.toasts.at(-1)).toMatchObject({ text: m.lamps_all_off_done({ count: 2 }), warn: false });
		sayOutcome({ done: ['A'], failed: ['B', 'C'] }, (count) => m.lamps_all_off_done({ count }));
		expect(ui.toasts.at(-1)).toMatchObject({
			text: m.group_partial({ done: m.lamps_all_off_done({ count: 1 }), names: list(['B', 'C'], 'long') }),
			warn: true
		});
	});
});
