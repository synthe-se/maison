import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { html } from '#lib/test/snippet.ts';
import ListRow from './ListRow.svelte';

describe('ListRow', () => {
	it('a list item: its name, the facts under it, its actions', async () => {
		await render(ListRow, {
			children: html('<span>iPhone</span>'),
			second: 'Créée le 2 octobre 2026',
			end: html('<button>Renommer</button>')
		});
		const row = page.getByRole('listitem');
		await expect.element(row.getByText('iPhone')).toBeVisible();
		await expect.element(row.getByText('Créée le 2 octobre 2026')).toBeVisible();
		await expect.element(row.getByRole('button', { name: 'Renommer' })).toBeVisible();
	});

	it('`whole` in place of the row (a name edited in place)', async () => {
		await render(ListRow, { children: html('<span>iPhone</span>'), whole: html('<input aria-label="Nom" />') });
		await expect.element(page.getByRole('textbox', { name: 'Nom' })).toBeVisible();
		await expect.element(page.getByText('iPhone')).not.toBeInTheDocument();
	});
});
