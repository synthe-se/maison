import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import Toasts from './Toasts.svelte';

describe('Toasts', () => {
	afterEach(() => {
		for (const t of ui.toasts.slice()) ui.dismiss(t.id);
		vi.useRealTimers();
	});

	it('shows each message, and closes it from its button', async () => {
		await render(Toasts);
		ui.toast('Programme enregistré');
		ui.toast('Erreur. TV injoignable', { warn: true });
		await expect.element(page.getByText('Programme enregistré')).toBeVisible();
		await expect.element(page.getByText('Erreur. TV injoignable')).toBeVisible();
		await page.getByRole('button', { name: m.dismiss() }).first().click();
		await expect.element(page.getByText('Programme enregistré')).not.toBeInTheDocument();
		await expect.element(page.getByText('Erreur. TV injoignable')).toBeVisible();
	});

	it('lets a message go after 5 s, but not while the pointer or focus is on it', async () => {
		vi.useFakeTimers();
		const { container } = await render(Toasts);
		ui.toast('Enregistré');
		flushSync();
		const toast = container.querySelector('.toast')!;
		toast.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
		toast.dispatchEvent(new MouseEvent('mouseenter'));
		vi.advanceTimersByTime(10_000);
		flushSync();
		expect(container.textContent).toContain('Enregistré');
		toast.dispatchEvent(new MouseEvent('mouseleave'));
		toast.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		vi.advanceTimersByTime(10_000);
		flushSync();
		expect(container.textContent).toContain('Enregistré');
		toast.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
		vi.advanceTimersByTime(5_000);
		flushSync();
		expect(container.querySelector('.toast')).toBeNull();
	});

	it('keeps an error until it is read and closed', async () => {
		vi.useFakeTimers();
		const { container } = await render(Toasts);
		ui.toast('Erreur', { warn: true });
		vi.advanceTimersByTime(60_000);
		flushSync();
		expect(container.querySelector('.toast.warn')).not.toBeNull();
	});

	it('an action: its button does it, the toast leaves, the focus goes where the action says', async () => {
		await render(Toasts);
		const back = document.createElement('button');
		back.textContent = 'Ajouter';
		document.body.append(back);
		const run = vi.fn();
		const id = ui.toast('Repas supprimé', { action: { label: 'Rétablir', run, back: () => back } });
		const restore = page.getByRole('button', { name: 'Rétablir' });
		await expect.element(restore).toHaveAttribute('id', `toast-action-${id}`);
		(restore.element() as HTMLElement).focus();
		await restore.click();
		expect(run).toHaveBeenCalledOnce();
		await expect.element(page.getByText('Repas supprimé')).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: 'Ajouter' })).toHaveFocus();
		back.remove();
	});

	it('a toast with an action stays 10 s, the time to reach it', async () => {
		vi.useFakeTimers();
		const { container } = await render(Toasts);
		ui.toast('Repas supprimé', { action: { label: 'Rétablir', run: () => {} } });
		vi.advanceTimersByTime(9_000);
		flushSync();
		expect(container.textContent).toContain('Repas supprimé');
		vi.advanceTimersByTime(1_000);
		flushSync();
		expect(container.querySelector('.toast')).toBeNull();
	});
});
