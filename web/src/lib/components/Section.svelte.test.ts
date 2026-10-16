import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { text } from '#lib/test/snippet.ts';
import Section from './Section.svelte';

describe('Section', () => {
	it('is a region named by its heading, holding its rows', async () => {
		await render(Section, { title: 'Distribution', icon: 'utensils', children: text('Contenu') });
		const region = page.getByRole('region', { name: 'Distribution' });
		await expect.element(region).toBeVisible();
		await expect.element(page.getByRole('heading', { level: 2, name: 'Distribution' })).toBeVisible();
		await expect.element(region.getByText('Contenu')).toBeVisible();
	});

	it('goes without an icon', async () => {
		const { container } = await render(Section, { title: 'État', children: text('x') });
		await expect.element(page.getByRole('heading', { name: 'État' })).toBeVisible();
		expect(container.querySelector('h2 svg')).toBeNull();
	});

	it('bare, or a smaller title inside a block, its title able to take the focus', async () => {
		const { container } = await render(Section, { title: 'Effets', tile: false, level: 3, children: text('x') });
		const heading = page.getByRole('heading', { level: 3, name: 'Effets' });
		await expect.element(heading).toHaveAttribute('tabindex', '-1');
		expect(container.querySelector('section')?.classList.contains('tile')).toBe(false);
	});
});
