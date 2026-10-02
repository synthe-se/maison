import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Icon from './Icon.svelte';

describe('Icon', () => {
	it('is decoration without a label: hidden from readers', async () => {
		const { container } = await render(Icon, { name: 'house' });
		const svg = container.querySelector('svg')!;
		expect(svg.getAttribute('aria-hidden')).toBe('true');
		expect(svg.getAttribute('role')).toBeNull();
		expect(svg.getAttribute('width')).toBe('18');
		expect(svg.querySelector('path')).not.toBeNull();
	});

	it('speaks as an image when labelled', async () => {
		await render(Icon, { name: 'circle-alert', label: 'Injoignable', size: 24, class: 'warn' });
		const img = page.getByRole('img', { name: 'Injoignable' });
		await expect.element(img).toBeVisible();
		await expect.element(img).not.toHaveAttribute('aria-hidden');
		await expect.element(img).toHaveAttribute('width', '24');
		await expect.element(img).toHaveClass('warn');
	});
});
