import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Toggle from './Toggle.svelte';

describe('Toggle', () => {
	it('is a switch named by its label, showing the device’s state', async () => {
		await render(Toggle, { label: 'Veille', checked: true, onchange: () => {} });
		await expect.element(page.getByRole('switch', { name: 'Veille' })).toBeChecked();
	});

	it('asks for the other state, and does not flip on its own', async () => {
		const onchange = vi.fn();
		await render(Toggle, { label: 'Veille', checked: false, onchange });
		const sw = page.getByRole('switch', { name: 'Veille' });
		await sw.click();
		expect(onchange).toHaveBeenCalledExactlyOnceWith(true);
	});

	it('works from the keyboard', async () => {
		const onchange = vi.fn();
		await render(Toggle, { label: 'Veille', checked: true, onchange });
		(document.querySelector('[role=switch]') as HTMLElement).focus();
		await userEvent.keyboard(' ');
		expect(onchange).toHaveBeenCalledWith(false);
	});

	it('is busy and cannot be pressed twice while the command travels', async () => {
		const onchange = vi.fn();
		await render(Toggle, { label: 'Veille', checked: false, onchange, pending: true });
		const sw = page.getByRole('switch', { name: 'Veille' });
		await expect.element(sw).toHaveAttribute('aria-busy', 'true');
		await expect.element(sw).toHaveAttribute('aria-disabled', 'true');
		// a real press: Playwright itself refuses to click what says it is disabled
		(sw.element() as HTMLElement).click();
		expect(onchange).not.toHaveBeenCalled();
		await expect.element(sw).not.toBeChecked();
	});

	it('keeps the focus while its command travels (never disabled under the finger)', async () => {
		const { rerender } = await render(Toggle, { label: 'Veille', checked: false, onchange: () => {} });
		const sw = document.querySelector<HTMLElement>('[role=switch]')!;
		sw.focus();
		await rerender({ pending: true });
		expect(document.activeElement).toBe(sw);
		expect(sw.hasAttribute('disabled')).toBe(false);
	});

	it('can be disabled, and keep its label for readers only', async () => {
		await render(Toggle, { label: 'Veille', checked: false, onchange: () => {}, disabled: true, hideLabel: true });
		await expect.element(page.getByRole('switch', { name: 'Veille' })).toBeDisabled();
		await expect.element(page.getByText('Veille')).toHaveClass('sr-only');
	});
});
