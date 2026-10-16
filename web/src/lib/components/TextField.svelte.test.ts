import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import TextField from './TextField.svelte';

describe('TextField', () => {
	it('a labelled input, its attributes passed through', async () => {
		await render(TextField, { label: 'Nom', value: 'Salon', maxlength: 60, autocomplete: 'off' });
		const field = page.getByRole('textbox', { name: 'Nom' });
		await expect.element(field).toHaveValue('Salon');
		await expect.element(field).toHaveAttribute('maxlength', '60');
		await expect.element(field).not.toHaveAttribute('aria-invalid');
	});

	it('what is wrong is said under it, the input described by it and marked invalid', async () => {
		await render(TextField, { label: 'Code', value: '', error: 'Le code manque' });
		const field = page.getByRole('textbox', { name: 'Code' });
		await expect.element(field).toHaveAttribute('aria-invalid', 'true');
		await expect.element(field).toHaveAccessibleDescription('Le code manque');
	});
});
