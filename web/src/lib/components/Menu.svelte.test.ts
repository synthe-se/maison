import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import Menu from './Menu.svelte';

describe('Menu', () => {
	it('a button named by its label opens its sections: actions, and switches that keep it open', async () => {
		const refresh = vi.fn();
		const oncheck = vi.fn();
		await render(Menu, {
			label: 'Actions des chats',
			sections: [
				{ heading: 'Connexion locale', items: [{ label: 'Fontaine', checked: true, oncheck }] },
				{ items: [{ label: 'Actualiser', icon: 'refresh-cw', onselect: refresh }] },
				{ heading: 'Vide', items: [] }
			]
		});
		const button = page.getByRole('button', { name: 'Actions des chats' });
		await button.click();
		const fountain = page.getByRole('menuitemcheckbox', { name: 'Fontaine' });
		await expect.element(fountain).toHaveAttribute('aria-checked', 'true');
		await expect.element(page.getByText('Connexion locale')).toBeVisible();
		// an empty section is left out
		expect(page.getByText('Vide').query()).toBeNull();
		await fountain.click();
		expect(oncheck).toHaveBeenCalledWith(false);
		// a switch keeps the menu open
		await page.getByRole('menuitem', { name: 'Actualiser' }).click();
		expect(refresh).toHaveBeenCalledOnce();
		await expect.element(page.getByRole('menu')).not.toBeInTheDocument();
	});

	it('says it is busy while an item’s gesture travels, and keeps the focus', async () => {
		await render(Menu, { label: 'Ajouter une lampe', icon: 'plus', busy: true, sections: [] });
		await expect.element(page.getByRole('button', { name: 'Ajouter une lampe' })).toHaveAttribute('aria-busy', 'true');
	});
});
