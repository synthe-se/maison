import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { html } from '#lib/test/snippet.ts';
import Sheet from './Sheet.svelte';

const body = html('<label>Nom <input /></label>');

describe('Sheet', () => {
	it('is a dialog named by its title, described, with its form inside', async () => {
		await render(Sheet, { open: true, onclose: () => {}, title: 'Nouveau repas', description: 'Pour le bébé', children: body });
		const dialog = page.getByRole('dialog', { name: 'Nouveau repas' });
		await expect.element(dialog).toBeVisible();
		await expect.element(dialog).toHaveAccessibleDescription('Pour le bébé');
		await expect.element(dialog.getByRole('heading', { level: 2, name: 'Nouveau repas' })).toBeVisible();
		await expect.element(dialog.getByRole('textbox', { name: 'Nom' })).toBeVisible();
	});

	it('draws nothing while closed', async () => {
		await render(Sheet, { open: false, onclose: () => {}, title: 'Nouveau repas', children: body });
		await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
	});

	it('closes from the cross', async () => {
		const onclose = vi.fn();
		await render(Sheet, { open: true, onclose, title: 'Liaison', children: body });
		await page.getByRole('button', { name: m.dismiss() }).click();
		expect(onclose).toHaveBeenCalledOnce();
	});

	it('closes from Escape', async () => {
		const onclose = vi.fn();
		await render(Sheet, { open: true, onclose, title: 'Liaison', children: body });
		await expect.element(page.getByRole('dialog')).toBeVisible();
		await userEvent.keyboard('{Escape}');
		expect(onclose).toHaveBeenCalledOnce();
	});
});

describe('Sheet with unsaved changes', () => {
	it('asks before Escape loses them, and keeps the form on « Continuer »', async () => {
		const onclose = vi.fn();
		await render(Sheet, { open: true, onclose, title: 'Liaison', dirty: true, children: body });
		await expect.element(page.getByRole('dialog')).toBeVisible();
		await userEvent.keyboard('{Escape}');
		const ask = page.getByRole('alertdialog', { name: m.sheet_discard_title() });
		await expect.element(ask).toBeVisible();
		await ask.getByRole('button', { name: m.sheet_keep_editing() }).click();
		await expect.element(ask).not.toBeInTheDocument();
		expect(onclose).not.toHaveBeenCalled();
		await expect.element(page.getByRole('textbox', { name: 'Nom' })).toBeVisible();
	});

	it('closes once the changes are given up', async () => {
		const onclose = vi.fn();
		await render(Sheet, { open: true, onclose, title: 'Liaison', dirty: true, children: body });
		await page.getByRole('button', { name: m.dismiss() }).click();
		await page.getByRole('alertdialog').getByRole('button', { name: m.sheet_discard() }).click();
		expect(onclose).toHaveBeenCalledOnce();
	});
});
