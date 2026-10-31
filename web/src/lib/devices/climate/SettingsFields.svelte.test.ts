import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import SettingsFields from './SettingsFields.svelte';
import { MODE_LABEL } from './labels.ts';

const cool = { mode: 'cool', temperature: 20, fan: 'auto', vane: 'auto', econo: true } as const;

describe('SettingsFields', () => {
	it('switching out of cooling turns econo off', async () => {
		const onchange = vi.fn();
		await render(SettingsFields, { settings: cool, onchange });
		await page.getByRole('combobox', { name: new RegExp(`^${m.climate_mode()}`) }).click();
		await page.getByRole('option', { name: MODE_LABEL.heat(), exact: true }).click();
		expect(onchange).toHaveBeenLastCalledWith({ ...cool, mode: 'heat', econo: false });
	});

	it('econo stays in place outside cooling, unavailable, saying why', async () => {
		await render(SettingsFields, { settings: { ...cool, mode: 'heat', econo: false }, onchange: () => {} });
		const econo = page.getByRole('switch', { name: m.climate_econo_cool() });
		await expect.element(econo).toHaveAttribute('aria-disabled', 'true');
		await expect.element(econo).toHaveAccessibleDescription(m.climate_econo_cool_description());
	});

	it('the temperature said in words, one degree a key', async () => {
		const onchange = vi.fn();
		await render(SettingsFields, { settings: cool, onchange });
		const slider = page.getByRole('slider', { name: m.climate_temperature() });
		await expect.element(slider).toHaveAttribute('aria-valuetext', m.climate_degrees_words({ degrees: 20 }));
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		expect(onchange).toHaveBeenLastCalledWith({ ...cool, temperature: 21 });
	});
});
