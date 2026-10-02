import { describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import SettingRow from './SettingRow.svelte';

describe('SettingRow', () => {
	it('is a list row with a named switch and a hint under it', async () => {
		const onchange = vi.fn();
		await render(SettingRow, { icon: 'bell-off', label: 'Silence la nuit', checked: true, onchange, hint: 'De 22 h à 7 h' });
		expect(document.querySelector('li.setting')).not.toBeNull();
		const sw = page.getByRole('switch', { name: 'Silence la nuit' });
		await expect.element(sw).toBeChecked();
		await expect.element(page.getByText('De 22 h à 7 h')).toBeVisible();
		await sw.click();
		expect(onchange).toHaveBeenCalledWith(false);
	});

	it('passes pending and disabled to its switch, and draws no hint without one', async () => {
		await render(SettingRow, { icon: 'bell-off', label: 'Silence', checked: false, onchange: () => {}, pending: true });
		await expect.element(page.getByRole('switch', { name: 'Silence' })).toHaveAttribute('aria-busy', 'true');
		expect(document.querySelector('.hint')).toBeNull();
	});
});
