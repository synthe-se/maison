import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Progress from './Progress.svelte';

describe('Progress', () => {
	it('is a progress bar named by its label, its value said in words', async () => {
		await render(Progress, { label: 'Réveil de la TV', value: 12.4, max: 30, valueText: '12 s sur 30' });
		const bar = page.getByRole('progressbar', { name: 'Réveil de la TV' });
		await expect.element(bar).toHaveAttribute('aria-valuenow', '12');
		await expect.element(bar).toHaveAttribute('aria-valuemin', '0');
		await expect.element(bar).toHaveAttribute('aria-valuemax', '30');
		await expect.element(bar).toHaveAttribute('aria-valuetext', '12 s sur 30');
		await expect.element(page.getByText('12 s sur 30')).toBeVisible();
	});

	it('never goes past full', async () => {
		await render(Progress, { label: 'Envoi', value: 45, max: 30, valueText: 'fini' });
		await expect.element(page.getByRole('progressbar', { name: 'Envoi' })).toHaveAttribute('aria-valuenow', '30');
	});
});
