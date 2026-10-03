import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { html } from '#lib/test/snippet.ts';
import DeviceTile from './DeviceTile.svelte';

const settings = html('<div><label>Adresse <input /></label></div>');

describe('a tile’s settings (TileSettings)', () => {
	it('a gear named after the device, saying whether they are open, controlling them; they exist only while open', async () => {
		await render(DeviceTile, { name: 'TV du salon', state: 'Allumée', settings });
		const gear = page.getByRole('button', { name: m.common_settings_of({ name: 'TV du salon' }) });
		await expect.element(gear).toHaveAttribute('aria-expanded', 'false');
		await expect.element(page.getByRole('textbox', { name: 'Adresse' })).not.toBeInTheDocument();
		await gear.click();
		await expect.element(gear).toHaveAttribute('aria-expanded', 'true');
		await expect.element(page.getByRole('textbox', { name: 'Adresse' })).toBeVisible();
		expect(document.getElementById(gear.element().getAttribute('aria-controls')!)?.contains(page.getByRole('textbox').element())).toBe(
			true
		);
	});

	it('closed while the focus was in them, the focus goes back to the gear', async () => {
		const { rerender } = await render(DeviceTile, { name: 'TV du salon', state: 'Allumée', settings, settingsOpen: true });
		await page.getByRole('textbox', { name: 'Adresse' }).click();
		await userEvent.keyboard('a');
		await rerender({ settingsOpen: false });
		await expect.element(page.getByRole('button', { name: m.common_settings_of({ name: 'TV du salon' }) })).toHaveFocus();
	});
});
