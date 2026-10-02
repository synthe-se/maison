import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import StatusRow from './StatusRow.svelte';

describe('StatusRow', () => {
	it('pairs a label with its value in words', async () => {
		await render(StatusRow, { icon: 'battery', label: 'Batterie', value: '80 %' });
		await expect.element(page.getByRole('term')).toHaveTextContent('Batterie');
		await expect.element(page.getByRole('definition')).toHaveTextContent('80 %');
	});

	it('marks what needs a hand with a warning sign, not the colour alone', async () => {
		const { container } = await render(StatusRow, { icon: 'utensils', label: 'Croquettes', value: 'Bas', warn: true });
		await expect.element(page.getByRole('definition')).toHaveTextContent('Bas');
		expect(container.querySelector('.fact-row')?.classList.contains('warn')).toBe(true);
		expect(container.querySelectorAll('dd svg')).toHaveLength(1);
	});

	it('shows no warning sign otherwise', async () => {
		const { container } = await render(StatusRow, { icon: 'utensils', label: 'Croquettes', value: 'Plein' });
		await expect.element(page.getByRole('definition')).toHaveTextContent('Plein');
		expect(container.querySelectorAll('dd svg')).toHaveLength(0);
	});
});
