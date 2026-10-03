import '../../../styles/app.css';
import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { text } from '#lib/test/snippet.ts';
import type { TempoColor } from './colors.ts';
import Swatch from './Swatch.svelte';

async function mark(props: {
	color: TempoColor | null;
	forecast?: boolean;
	unsure?: boolean;
	size?: 'dot' | 'tile' | 'cell';
	current?: boolean;
}) {
	const { container } = await render(Swatch, props);
	const el = container.querySelector<HTMLElement>('.swatch')!;
	return { el, style: getComputedStyle(el) };
}

describe('Swatch', () => {
	it('is hidden from assistive technology: the color’s name is always said beside it', async () => {
		const { el } = await mark({ color: 'BLUE' });
		expect(el.getAttribute('aria-hidden')).toBe('true');
	});

	it('draws blue filled, white as a ring, red filled and hatched', async () => {
		const blue = await mark({ color: 'BLUE' });
		expect(blue.style.backgroundColor).not.toBe('rgba(0, 0, 0, 0)');
		expect(blue.style.backgroundImage).toBe('none');
		expect(blue.style.borderStyle).toBe('solid');

		const red = await mark({ color: 'RED' });
		expect(red.style.backgroundImage).toContain('repeating-linear-gradient');

		const white = await mark({ color: 'WHITE' });
		expect(white.style.backgroundImage).toBe('none');
		expect(white.style.borderStyle).toBe('solid');
		// three colors, three different marks
		expect(new Set([blue.style.backgroundColor, white.style.backgroundColor])).toHaveProperty('size', 2);
	});

	it('draws a forecast dashed, red still hatched, and an unknown day as a dashed ring', async () => {
		const forecast = await mark({ color: 'BLUE', forecast: true });
		expect(forecast.style.borderStyle).toBe('dashed');
		const red = await mark({ color: 'RED', forecast: true });
		expect(red.style.borderStyle).toBe('dashed');
		expect(red.style.backgroundImage).toContain('repeating-linear-gradient');
		const unknown = await mark({ color: null });
		expect(unknown.style.borderStyle).toBe('dashed');
		expect(unknown.style.backgroundColor).toBe('rgba(0, 0, 0, 0)');
	});

	it('draws a forecast under 60 % dotted, without a wash, red still hatched', async () => {
		const blue = await mark({ color: 'BLUE', forecast: true, unsure: true });
		expect(blue.style.borderStyle).toBe('dotted');
		expect(blue.style.backgroundColor).toBe('rgba(0, 0, 0, 0)');
		const red = await mark({ color: 'RED', forecast: true, unsure: true });
		expect(red.style.borderStyle).toBe('dotted');
		expect(red.style.backgroundImage).toContain('repeating-linear-gradient');
		// a published day is never « unsure »
		expect((await mark({ color: 'BLUE', unsure: true })).style.borderStyle).toBe('solid');
	});

	it('outlines today, and holds a calendar cell’s content', async () => {
		const { container } = await render(Swatch, { color: 'WHITE', size: 'cell', current: true, children: text('12') });
		const el = container.querySelector<HTMLElement>('.swatch')!;
		expect(getComputedStyle(el).outlineStyle).toBe('solid');
		expect(el.textContent).toBe('12');
		const plain = await mark({ color: 'WHITE', size: 'cell' });
		expect(plain.style.outlineStyle).toBe('none');
	});
});
