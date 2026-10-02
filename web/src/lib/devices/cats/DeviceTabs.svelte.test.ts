import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { text } from '#lib/test/snippet.ts';
import DeviceTabs from './DeviceTabs.svelte';

describe('DeviceTabs', () => {
	const props = () => ({ other: { label: 'Planning', icon: 'calendar' as const }, controls: text('Les commandes'), second: text('Le planning') });

	it('opens on the controls', async () => {
		await render(DeviceTabs, props());
		await expect.element(page.getByRole('tab', { name: m.device_tab_control() })).toHaveAttribute('aria-selected', 'true');
		await expect.element(page.getByText('Les commandes')).toBeVisible();
		await expect.element(page.getByRole('tab', { name: 'Planning' })).toHaveAttribute('aria-selected', 'false');
	});

	it('shows the second view when its tab is chosen, by click or by arrow key', async () => {
		await render(DeviceTabs, props());
		await page.getByRole('tab', { name: 'Planning' }).click();
		await expect.element(page.getByText('Le planning')).toBeVisible();
		await page.getByRole('tab', { name: m.device_tab_control() }).click();
		await expect.element(page.getByText('Les commandes')).toBeVisible();
		await userEvent.keyboard('{ArrowRight}');
		await expect.element(page.getByRole('tab', { name: 'Planning' })).toHaveFocus();
	});
});
