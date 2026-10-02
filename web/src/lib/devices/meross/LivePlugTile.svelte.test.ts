import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { forgetAll } from '#lib/live.svelte.ts';
import { json, stubFetch } from '#lib/test/fetch.ts';
import LivePlugTile from './LivePlugTile.svelte';
import { watts } from './units.ts';
import { merossElectricity, merossPlug } from '#lib/test/meross.ts';

afterEach(() => forgetAll());

describe('LivePlugTile', () => {
	it('reads the plug’s power once and shows it on the tile', async () => {
		const calls = stubFetch(() => json(merossElectricity()));
		await render(LivePlugTile, { plug: merossPlug() });
		await expect.element(page.getByText(watts(42.4))).toBeVisible();
		expect(calls.map((c) => c.url)).toEqual(['/api/meross/p1/electricity']);
		await expect.element(page.getByRole('button', { name: 'Radiateur' })).toHaveAttribute('aria-pressed', 'true');
	});
});
