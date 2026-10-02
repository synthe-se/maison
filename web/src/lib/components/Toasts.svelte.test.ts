import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import Toasts from './Toasts.svelte';

describe('Toasts', () => {
	afterEach(() => {
		for (const t of [...ui.toasts]) ui.dismiss(t.id);
		vi.useRealTimers();
	});

	it('shows each message, and closes it from its button', async () => {
		await render(Toasts);
		ui.toast('Programme enregistré');
		ui.toast('Erreur. TV injoignable', true);
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
		ui.toast('Erreur', true);
		vi.advanceTimersByTime(60_000);
		flushSync();
		expect(container.querySelector('.toast.warn')).not.toBeNull();
	});
});
