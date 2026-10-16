import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { html, text } from '#lib/test/snippet.ts';
import Door from './Door.svelte';

describe('Door', () => {
	it('a region named by its h1, which can take the focus, and the window titled after it', async () => {
		await render(Door, { title: 'Bon retour', children: text('Ta clé') });
		const region = page.getByRole('region', { name: 'Bon retour' });
		await expect.element(region).toBeVisible();
		await expect.element(page.getByRole('heading', { level: 1, name: 'Bon retour' })).toHaveAttribute('tabindex', '-1');
		await expect.element(region.getByText('Ta clé')).toBeVisible();
		await expect.poll(() => document.title).toBe(`Bon retour${m.facts_separator()}${m.branding_name()}`);
	});

	it('draws a mark above its title', async () => {
		const { container } = await render(Door, { title: 'C’est fait', mark: html('<p class="mark">✓</p>') });
		expect(container.querySelector('.mark')?.nextElementSibling?.tagName).toBe('H1');
	});
});
