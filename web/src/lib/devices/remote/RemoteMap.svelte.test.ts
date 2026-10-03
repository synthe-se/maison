import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import RemoteMap from './RemoteMap.svelte';
import { REMOTE_ROWS } from './keys.ts';

const keymap = {
	'116': { actions: [], label: 'Tout éteindre' },
	'5': { actions: [] }
};

describe('RemoteMap', () => {
	it('has one button per key, each saying whether it is configured', async () => {
		await render(RemoteMap, { keymap, onselect: () => {} });
		await expect.element(page.getByRole('button')).toHaveLength(REMOTE_ROWS.flat().length);
		await expect
			.element(page.getByRole('button', { name: m.remote_key_mapped({ key: m.key_power(), label: 'Tout éteindre' }) }))
			.toBeVisible();
		// configured without a label: the key's own name
		await expect.element(page.getByRole('button', { name: m.remote_key_mapped({ key: '4', label: '4' }) })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.remote_key_free({ key: m.key_volume_up() }) })).toBeVisible();
	});

	it('shows what is printed on a key that has no icon', async () => {
		await render(RemoteMap, { keymap: {}, onselect: () => {} });
		await expect.element(page.getByRole('button', { name: m.remote_key_free({ key: 'OK' }) })).toHaveTextContent('OK');
		await expect.element(page.getByRole('button', { name: m.remote_key_free({ key: m.key_guide() }) })).toHaveTextContent('EPG');
	});

	it('opens a key’s binding by click or keyboard', async () => {
		const onselect = vi.fn();
		await render(RemoteMap, { keymap, onselect });
		await page.getByRole('button', { name: m.remote_key_free({ key: 'OK' }) }).click();
		expect(onselect).toHaveBeenCalledWith(353);
		(page.getByRole('button', { name: m.remote_key_free({ key: m.key_up() }) }).element() as HTMLElement).focus();
		await userEvent.keyboard('{Enter}');
		expect(onselect).toHaveBeenLastCalledWith(103);
	});

	it('explains the legend and how to start', async () => {
		await render(RemoteMap, { keymap, onselect: () => {} });
		await expect.element(page.getByText(m.remote_visual_hint())).toBeVisible();
		await expect.element(page.getByText(m.remote_legend_mapped())).toBeVisible();
	});
});
