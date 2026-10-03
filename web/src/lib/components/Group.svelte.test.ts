import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { html } from '#lib/test/snippet.ts';
import type { Loadable } from '#lib/live.svelte.ts';
import Group from './Group.svelte';

describe('Group', () => {
	it('a region named by its title, which can take the focus; its fact and actions in the head', async () => {
		await render(Group, {
			id: 'lamps-title',
			title: 'Lampes',
			fact: '3 lampes',
			actions: html('<button>Tout éteindre</button>'),
			children: html('<p>Les lampes</p>')
		});
		const region = page.getByRole('region', { name: 'Lampes' });
		await expect.element(region.getByRole('heading', { level: 2, name: 'Lampes' })).toHaveAttribute('id', 'lamps-title');
		await expect.element(region.getByRole('heading', { level: 2 })).toHaveAttribute('tabindex', '-1');
		await expect.element(region.getByText('3 lampes')).toBeVisible();
		await expect.element(region.getByRole('button', { name: 'Tout éteindre' })).toBeVisible();
		await expect.element(region.getByText('Les lampes')).toBeVisible();
	});

	it('a value read late is said under the head, with a retry', async () => {
		const value: Loadable = { loading: false, failed: false, stale: true, at: Date.now(), error: undefined, refresh: async () => {} };
		await render(Group, { id: 'x', title: 'Clim', value, children: html('<p>x</p>') });
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
	});
});
