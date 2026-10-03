import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import Combobox from './Combobox.svelte';

describe('Combobox', () => {
	it('a labelled field whose matches are a listbox; picking one hands its value on', async () => {
		const onpick = vi.fn();
		await render(Combobox, { label: 'Ville', query: '', options: [{ value: 'lyon', label: 'Lyon' }], onpick, open: true });
		const field = page.getByRole('combobox', { name: 'Ville' });
		await expect.element(field).toBeVisible();
		await page.getByRole('option', { name: 'Lyon' }).click();
		expect(onpick).toHaveBeenCalledWith('lyon');
	});

	it('what is typed goes back to the caller (it searches)', async () => {
		let query = '';
		await render(Combobox, {
			label: 'Ville',
			get query() {
				return query;
			},
			set query(v: string) {
				query = v;
			},
			options: [],
			onpick: () => {}
		});
		await userEvent.fill(page.getByRole('combobox', { name: 'Ville' }), 'Lyo');
		expect(query).toBe('Lyo');
	});
});
