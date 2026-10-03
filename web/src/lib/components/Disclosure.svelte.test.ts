import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { html } from '#lib/test/snippet.ts';
import Disclosure from './Disclosure.svelte';

describe('Disclosure', () => {
	it('a button saying whether it is open, controlling its content, which exists only while open', async () => {
		await render(Disclosure, { label: 'Commande générée', children: html('<code>state-off</code>') });
		const button = page.getByRole('button', { name: 'Commande générée' });
		await expect.element(button).toHaveAttribute('aria-expanded', 'false');
		await expect.element(page.getByText('state-off')).not.toBeInTheDocument();
		await button.click();
		await expect.element(button).toHaveAttribute('aria-expanded', 'true');
		await expect.element(page.getByText('state-off')).toBeVisible();
		const controlled = document.getElementById(button.element().getAttribute('aria-controls')!);
		expect(controlled?.textContent).toContain('state-off');
	});
});
