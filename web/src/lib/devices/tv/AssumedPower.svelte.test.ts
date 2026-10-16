import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { Gesture } from '#lib/gesture.svelte.ts';
import AssumedPower from './AssumedPower.svelte';
import PairedChip from './PairedChip.svelte';
import type { TvRemote } from './tv.svelte.ts';

describe('AssumedPower', () => {
	it('« Allumer » and « Éteindre », each its order, busy while it travels', async () => {
		const g = new Gesture();
		const assumedPower = vi.fn((on: boolean) => g.run(() => new Promise(() => {}), undefined, on ? 'on' : 'off'));
		await render(AssumedPower, { tv: { g, assumedPower } as unknown as TvRemote });
		await page.getByRole('button', { name: m.action_turn_on() }).click();
		expect(assumedPower).toHaveBeenCalledWith(true);
		await expect.element(page.getByRole('button', { name: m.action_turn_on() })).toHaveAttribute('aria-busy', 'true');
		await expect.element(page.getByRole('button', { name: m.action_turn_off() })).not.toHaveAttribute('aria-busy');
	});
});

describe('PairedChip', () => {
	it('says the keys take Remote v2, and nothing over ADB', async () => {
		const { rerender } = await render(PairedChip, { paired: true });
		await expect.element(page.getByText(m.android_tv_paired())).toBeVisible();
		await rerender({ paired: false });
		await expect.element(page.getByText(m.android_tv_paired())).not.toBeInTheDocument();
	});
});
