import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { text } from '#lib/test/snippet.ts';
import PageHead from './PageHead.svelte';

describe('PageHead', () => {
	it('titles the window « X · Maison » and the page with one h1', async () => {
		await render(PageHead, { title: 'Salon' });
		await expect.element(page.getByRole('heading', { level: 1, name: 'Salon' })).toBeVisible();
		expect(document.querySelectorAll('h1')).toHaveLength(1);
		await expect.poll(() => document.title).toBe(`Salon · ${m.branding_name()}`);
		await expect.element(page.getByRole('link', { name: m.back_home() })).not.toBeInTheDocument();
	});

	it('the home page is titled « Maison » alone', async () => {
		await render(PageHead, { title: m.branding_name() });
		await expect.poll(() => document.title).toBe(m.branding_name());
	});

	it('the h1 can take the focus after a navigation', async () => {
		await render(PageHead, { title: 'Salon' });
		const h1 = document.querySelector('h1')!;
		h1.focus();
		expect(document.activeElement).toBe(h1);
	});

	it('a device page has a way back home, a line under and actions', async () => {
		await render(PageHead, { title: 'Lampe', back: true, sub: text('Allumée, 80 %'), end: text('Réglages') });
		await expect.element(page.getByRole('link', { name: m.back_home() })).toHaveAttribute('href', '/');
		await expect.element(page.getByText('Allumée, 80 %')).toBeVisible();
		await expect.element(page.getByText('Réglages')).toBeVisible();
	});
});
