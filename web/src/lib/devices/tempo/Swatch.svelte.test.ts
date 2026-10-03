import '../../../styles/app.css';
import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { text } from '#lib/test/snippet.ts';
import type { TempoColor } from './colors.ts';
import Swatch from './Swatch.svelte';

async function mark(props: { color: TempoColor | null; forecast?: boolean; unsure?: boolean; size?: 'dot' | 'tile' | 'cell'; current?: boolean }) {
	const { container } = await render(Swatch, props);
	const el = container.querySelector<HTMLElement>('.swatch')!;
	return { el, style: getComputedStyle(el) };
}

describe('Swatch', () => {
	it('is hidden from assistive technology: the colour’s name is always said beside it', async () => {
		const { el } = await mark({ color: 'BLUE' });
		expect(el.getAttribute('aria-hidden')).toBe('true');
	});

	it('draws bleu filled, blanc as a ring, rouge filled and hatched', async () => {
		const bleu = await mark({ color: 'BLUE' });
		expect(bleu.style.backgroundColor).not.toBe('rgba(0, 0, 0, 0)');
		expect(bleu.style.backgroundImage).toBe('none');
		expect(bleu.style.borderStyle).toBe('solid');

		const rouge = await mark({ color: 'RED' });
		expect(rouge.style.backgroundImage).toContain('repeating-linear-gradient');

		const blanc = await mark({ color: 'WHITE' });
		expect(blanc.style.backgroundImage).toBe('none');
		expect(blanc.style.borderStyle).toBe('solid');
		// three colours, three different marks
		expect(new Set([bleu.style.backgroundColor, blanc.style.backgroundColor])).toHaveProperty('size', 2);
	});

	it('draws a forecast dashed, rouge still hatched, and an unknown day as a dashed ring', async () => {
		const forecast = await mark({ color: 'BLUE', forecast: true });
		expect(forecast.style.borderStyle).toBe('dashed');
		const rouge = await mark({ color: 'RED', forecast: true });
		expect(rouge.style.borderStyle).toBe('dashed');
		expect(rouge.style.backgroundImage).toContain('repeating-linear-gradient');
		const unknown = await mark({ color: null });
		expect(unknown.style.borderStyle).toBe('dashed');
		expect(unknown.style.backgroundColor).toBe('rgba(0, 0, 0, 0)');
	});

	it('draws a forecast under 60 % dotted, without a wash, rouge still hatched', async () => {
		const bleu = await mark({ color: 'BLUE', forecast: true, unsure: true });
		expect(bleu.style.borderStyle).toBe('dotted');
		expect(bleu.style.backgroundColor).toBe('rgba(0, 0, 0, 0)');
		const rouge = await mark({ color: 'RED', forecast: true, unsure: true });
		expect(rouge.style.borderStyle).toBe('dotted');
		expect(rouge.style.backgroundImage).toContain('repeating-linear-gradient');
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
