import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { text } from '#lib/test/snippet.ts';
import Settings from './Settings.svelte';

const fields = [
	{ key: 'host', label: m.tv_host(), placeholder: '192.168.1.52' },
	{ key: 'boxHost', label: m.tv_box_host(), placeholder: '192.168.1.153' }
];

describe('tv/Settings', () => {
	it('fills each labelled field from the saved values, shows the hint and its children', async () => {
		await render(Settings, {
			fields,
			initial: { host: '10.0.0.2', boxHost: null },
			hint: m.tv_configure_hint(),
			save: vi.fn(),
			saved: m.tv_saved(),
			children: text('Pairing')
		});
		await expect.element(page.getByLabelText(m.tv_host())).toHaveValue('10.0.0.2');
		await expect.element(page.getByLabelText(m.tv_box_host())).toHaveValue('');
		await expect.element(page.getByLabelText(m.tv_box_host())).toHaveAttribute('placeholder', '192.168.1.153');
		await expect.element(page.getByText(m.tv_configure_hint())).toBeVisible();
		await expect.element(page.getByText('Pairing')).toBeVisible();
	});

	it('saves the draft trimmed, empty fields as null, then says so and closes', async () => {
		const toast = vi.spyOn(ui, 'toast');
		const save = vi.fn(async () => {});
		const onsaved = vi.fn();
		await render(Settings, { fields, initial: { host: '10.0.0.2', boxHost: '10.0.0.3' }, save, saved: m.tv_saved(), onsaved });
		await userEvent.fill(page.getByLabelText(m.tv_host()).element(), '  10.0.0.9 ');
		await userEvent.fill(page.getByLabelText(m.tv_box_host()).element(), '   ');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.poll(() => onsaved.mock.calls.length).toBe(1);
		expect(save).toHaveBeenCalledExactlyOnceWith({ host: '10.0.0.9', boxHost: null });
		expect(toast).toHaveBeenCalledWith(m.tv_saved());
	});

	it('a refused save is said under the fields, the focus on the first, the settings stay open', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		let reject!: (e: Error) => void;
		const save = vi.fn(() => new Promise((_, r) => (reject = r)));
		const onsaved = vi.fn();
		await render(Settings, { fields, initial: {}, save, saved: m.tv_saved(), onsaved });
		const button = page.getByRole('button', { name: m.common_save() });
		await button.click();
		// busy: said so, still focusable
		await expect.element(button).toHaveAttribute('aria-busy', 'true');
		reject(new Error('TV unreachable'));
		await expect.element(button).not.toHaveAttribute('aria-busy');
		const first = page.getByLabelText(fields[0].label);
		await expect.element(first).toHaveAccessibleDescription('TV unreachable');
		await expect.element(first).toHaveFocus();
		expect(fail).not.toHaveBeenCalled();
		expect(onsaved).not.toHaveBeenCalled();
	});
});
