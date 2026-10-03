import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import ConfirmDialog from './ConfirmDialog.svelte';

const props = (over: Partial<Parameters<typeof render>[1]> = {}) => ({
	label: 'Remettre à zéro',
	title: 'Remettre le compteur à zéro ?',
	description: 'Le total depuis la mise en service sera perdu.',
	action: 'Remettre à zéro',
	onconfirm: vi.fn(),
	...over
});

describe('ConfirmDialog', () => {
	it('says the consequence, focuses the title, and offers « Garder » and the precise verb', async () => {
		const p = props();
		await render(ConfirmDialog, p);
		await page.getByRole('button', { name: 'Remettre à zéro' }).click();
		const dialog = page.getByRole('alertdialog', { name: p.title });
		await expect.element(dialog).toBeVisible();
		await expect.element(dialog).toHaveAccessibleDescription(p.description);
		await expect.element(dialog.getByRole('heading', { name: p.title })).toHaveFocus();
		await expect.element(dialog.getByRole('button', { name: m.device_keep() })).toBeVisible();
		await expect.element(dialog.getByRole('button', { name: p.action })).toBeVisible();
	});

	it('does it on the verb', async () => {
		const p = props();
		await render(ConfirmDialog, p);
		await page.getByRole('button', { name: 'Remettre à zéro' }).click();
		await page.getByRole('alertdialog').getByRole('button', { name: p.action }).click();
		expect(p.onconfirm).toHaveBeenCalledOnce();
	});

	it('closes once confirmed', async () => {
		const p = props();
		await render(ConfirmDialog, p);
		await page.getByRole('button', { name: 'Remettre à zéro' }).click();
		await page.getByRole('alertdialog').getByRole('button', { name: p.action }).click();
		await expect.element(page.getByRole('alertdialog'), { timeout: 500 }).not.toBeInTheDocument();
	});

	it('« Garder » and Escape change nothing', async () => {
		const p = props();
		await render(ConfirmDialog, p);
		await page.getByRole('button', { name: 'Remettre à zéro' }).click();
		await page.getByRole('button', { name: m.device_keep() }).click();
		await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
		await page.getByRole('button', { name: 'Remettre à zéro' }).click();
		await expect.element(page.getByRole('alertdialog')).toBeVisible();
		await userEvent.keyboard('{Escape}');
		await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
		expect(p.onconfirm).not.toHaveBeenCalled();
	});

	it('keeps the focus on its trigger when the action starts running', async () => {
		const { rerender } = await render(ConfirmDialog, props());
		const trigger = page.getByRole('button', { name: 'Remettre à zéro' }).element() as HTMLElement;
		trigger.focus();
		await rerender({ pending: true });
		expect(document.activeElement).toBe(trigger);
	});

	it('names its trigger precisely when asked', async () => {
		await render(ConfirmDialog, props({ ariaLabel: 'Retirer la clé MacBook', ghost: true }));
		await expect.element(page.getByRole('button', { name: 'Retirer la clé MacBook' })).toHaveClass('ghost');
	});

	it('cannot be opened while the action runs, or when disabled', async () => {
		const { rerender } = await render(ConfirmDialog, props({ pending: true, icon: 'refresh-cw', danger: true }));
		const trigger = page.getByRole('button', { name: 'Remettre à zéro' });
		// busy: still focusable, said busy, and a press opens nothing
		await expect.element(trigger).toHaveAttribute('aria-disabled', 'true');
		await expect.element(trigger).toHaveAttribute('aria-busy', 'true');
		(trigger.element() as HTMLElement).click();
		await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
		await rerender({ pending: false, disabled: true });
		await expect.element(trigger).toBeDisabled();
		await expect.element(trigger).toHaveClass('danger');
	});
});
