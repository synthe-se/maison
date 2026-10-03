import { describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Select from './Select.svelte';

/** The button that opens the list (APG select-only combobox: a button with a listbox popup). */
const trigger = () => page.getByRole('button', { name: /^Lampe/ });

const options = [
	{ value: 'salon', label: 'Lampe du salon' },
	{ value: 'cuisine', label: 'Lampe de la cuisine' }
];

describe('Select', () => {
	it('is named by its label and shows the chosen option', async () => {
		await render(Select, { label: 'Lampe', value: 'cuisine', options, onchange: () => {} });
		await expect.element(trigger()).toHaveAttribute('aria-haspopup', 'listbox');
		await expect.element(page.getByRole('button', { name: 'Lampe Lampe de la cuisine' })).toBeVisible();
	});

	it('shows the placeholder when nothing is chosen', async () => {
		await render(Select, { label: 'Lampe', value: '', options, onchange: () => {}, placeholder: 'Choisir…' });
		await expect.element(trigger()).toHaveTextContent('Choisir…');
	});

	it('offers the options and says the one picked', async () => {
		const onchange = vi.fn();
		await render(Select, { label: 'Lampe', value: 'salon', options, onchange });
		await trigger().click();
		await expect.element(page.getByRole('option', { name: 'Lampe du salon' })).toHaveAttribute('aria-selected', 'true');
		await page.getByRole('option', { name: 'Lampe de la cuisine' }).click();
		expect(onchange).toHaveBeenCalledWith('cuisine');
	});

	it('keeps a saved value no longer offered, under its own name', async () => {
		await render(Select, { label: 'Lampe', value: 'garage', options, onchange: () => {} });
		await expect.element(trigger()).toHaveTextContent('garage');
		await trigger().click();
		await expect.element(page.getByRole('option', { name: 'garage' })).toBeVisible();
		expect(page.getByRole('option').all()).toHaveLength(3);
	});

	it('can hide its label for sight only (still named by it)', async () => {
		await render(Select, { label: 'Lampe', value: 'salon', options, onchange: () => {}, hideLabel: true });
		await expect.element(page.getByText('Lampe', { exact: true })).toHaveClass('sr-only');
		await expect.element(page.getByRole('button', { name: 'Lampe Lampe du salon' })).toBeVisible();
	});
});
