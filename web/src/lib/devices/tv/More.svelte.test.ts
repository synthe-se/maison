import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { text } from '#lib/test/snippet.ts';
import More from './More.svelte';

const wide = (matches: boolean) => vi.spyOn(window, 'matchMedia').mockReturnValue({ matches } as MediaQueryList);

describe('tv/More', () => {
	it('folded on a phone; the button unfolds what it controls', async () => {
		wide(false);
		await render(More, { children: text('Pad') });
		const button = page.getByRole('button', { name: m.tv_more_controls() });
		await expect.element(button).toHaveAttribute('aria-expanded', 'false');
		const body = document.getElementById(button.element().getAttribute('aria-controls')!)!;
		expect(body.hidden).toBe(true);
		await button.click();
		await expect.element(button).toHaveAttribute('aria-expanded', 'true');
		expect(body.hidden).toBe(false);
		expect(body.textContent).toContain('Pad');
	});

	it('open from 600 px, and folds on a press', async () => {
		wide(true);
		await render(More, { children: text('Pad') });
		const button = page.getByRole('button', { name: m.tv_more_controls() });
		await expect.element(button).toHaveAttribute('aria-expanded', 'true');
		await button.click();
		await expect.element(button).toHaveAttribute('aria-expanded', 'false');
		// hidden (app.css makes `hidden` beat the component's display rule)
		expect(document.getElementById(button.element().getAttribute('aria-controls')!)!.hidden).toBe(true);
	});
});
