import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { html } from '#lib/test/snippet.ts';
import '../../styles/app.css';
import AuthShell from './AuthShell.svelte';

describe('AuthShell', () => {
	it('the identity (the mark, the name, the promise), then the page’s own content', async () => {
		const { container } = await render(AuthShell, {
			children: html('<section><h1>Entrer</h1><button class="btn primary big">Se connecter</button></section>')
		});
		await expect.element(page.getByText(m.branding_name(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.branding_promise())).toBeVisible();
		await expect.element(page.getByRole('heading', { level: 1, name: 'Entrer' })).toBeVisible();
		// the mark and the sun's path are decoration: nothing for a reader
		expect(container.querySelector('img.mark')?.getAttribute('alt')).toBe('');
		expect(container.querySelector('svg.trail')?.getAttribute('aria-hidden')).toBe('true');
		await expect.element(page.getByRole('img')).not.toBeInTheDocument();
	});

	it('one big action, 52 px tall', async () => {
		await render(AuthShell, { children: html('<section><button class="btn primary big">Se connecter</button></section>') });
		const big = page.getByRole('button', { name: 'Se connecter' }).element() as HTMLElement;
		expect(Math.round(big.getBoundingClientRect().height)).toBe(52);
	});

	it('one column on a phone, the identity beside the action from 840 px', async () => {
		const { container } = await render(AuthShell, { children: html('<section><h1>Entrer</h1></section>') });
		const door = container.querySelector('.door') as HTMLElement;
		const side = () => (container.querySelector('.side') as HTMLElement).getBoundingClientRect();
		const content = () => (container.querySelector('.content') as HTMLElement).getBoundingClientRect();
		door.style.width = '390px';
		// a phone's width does not trigger the media query in a wide test window: compare the layout the window gives
		const wide = innerWidth >= 840;
		const twoColumns = Math.round(content().left) > Math.round(side().left);
		const stacked = Math.round(content().top) >= Math.round(side().bottom);
		expect(wide ? twoColumns : stacked).toBe(true);
	});
});
