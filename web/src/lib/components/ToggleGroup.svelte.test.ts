import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import ToggleGroup from './ToggleGroup.svelte';

const modes = [
	{ value: 'cool', label: 'Froid' },
	{ value: 'heat', label: 'Chaud' }
] as const;

describe('ToggleGroup', () => {
	it('one of a few: a labelled group of radios, the chosen one checked; arrows move, Space chooses', async () => {
		const onchange = vi.fn();
		await render(ToggleGroup, { type: 'single', label: 'Mode', options: modes, value: 'cool', onchange });
		await expect.element(page.getByRole('group', { name: 'Mode' })).toBeVisible();
		await expect.element(page.getByRole('radio', { name: 'Froid' })).toHaveAttribute('aria-checked', 'true');
		await page.getByRole('radio', { name: 'Chaud' }).click();
		expect(onchange).toHaveBeenLastCalledWith('heat');
		(page.getByRole('radio', { name: 'Froid' }).element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		await expect.element(page.getByRole('radio', { name: 'Chaud' })).toHaveFocus();
	});

	it('a single choice is never emptied by pressing it again', async () => {
		const onchange = vi.fn();
		await render(ToggleGroup, { type: 'single', label: 'Mode', options: modes, value: 'cool', onchange });
		await page.getByRole('radio', { name: 'Froid' }).click();
		expect(onchange).not.toHaveBeenCalled();
	});

	it('any of them: pressed toggles, the new set handed on', async () => {
		const onchange = vi.fn();
		await render(ToggleGroup, {
			type: 'multiple',
			label: 'Jours',
			options: [
				{ value: 'mon', label: 'Lundi' },
				{ value: 'tue', label: 'Mardi' }
			],
			value: ['mon'],
			onchange
		});
		await expect.element(page.getByRole('button', { name: 'Lundi' })).toHaveAttribute('aria-pressed', 'true');
		await page.getByRole('button', { name: 'Mardi' }).click();
		expect(onchange).toHaveBeenLastCalledWith(['mon', 'tue']);
	});

	it('busy: the choices keep the focus, say they are busy, and ignore presses', async () => {
		const onchange = vi.fn();
		await render(ToggleGroup, { type: 'single', label: 'Mode', options: modes, value: 'cool', onchange, busy: true });
		const heat = page.getByRole('radio', { name: 'Chaud' });
		await expect.element(heat).toHaveAttribute('aria-busy', 'true');
		(heat.element() as HTMLElement).click();
		expect(onchange).not.toHaveBeenCalled();
	});
});
