import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { htmlOf } from '#lib/test/snippet.ts';
import Tabs from './Tabs.svelte';

const tabs = [
	{ value: 'white', label: 'Blanc', icon: 'lightbulb' as const },
	{ value: 'color', label: 'Couleur' }
];
const panel = htmlOf<string>((v) => `<p>Réglages ${v}</p>`);

describe('Tabs', () => {
	it('shows the chosen view, its tab selected', async () => {
		await render(Tabs, { tabs, value: 'white', panel });
		await expect.element(page.getByRole('tab', { name: 'Blanc' })).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByRole('tabpanel', { name: 'Blanc' })).toHaveTextContent('Réglages white');
		// the other view is `hidden` (app.css enforces it over the panels' own display)
		const other = [...document.querySelectorAll('[role=tabpanel]')].find((p) => p.textContent === 'Réglages color');
		expect(other?.hasAttribute('hidden')).toBe(true);
	});

	it('changes view on a click and says which', async () => {
		const onchange = vi.fn();
		await render(Tabs, { tabs, value: 'white', panel, onchange });
		await page.getByRole('tab', { name: 'Couleur' }).click();
		expect(onchange).toHaveBeenCalledWith('color');
		await expect.element(page.getByRole('tab', { name: 'Couleur' })).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByRole('tabpanel', { name: 'Couleur' })).toHaveTextContent('Réglages color');
	});

	it('moves between tabs with the arrow keys', async () => {
		await render(Tabs, { tabs, value: 'white', panel });
		(document.querySelector('[role=tab]') as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		await expect.element(page.getByRole('tab', { name: 'Couleur' })).toHaveFocus();
		await expect.element(page.getByRole('tab', { name: 'Couleur' })).toHaveAttribute('aria-selected', 'true');
	});
});
