import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { html } from '#lib/test/snippet.ts';
import { hueLamp } from '#lib/test/lamps.ts';
import LampGroup from './LampGroup.svelte';
import { fromHue, hue } from './lamp.ts';
import type { Live } from '#lib/live.svelte.ts';

/** A list as `live` gives it: read, still loading, or failed. */
const list = (state: Partial<Live<unknown>> = {}) => ({ loading: false, failed: false, data: {}, error: undefined, refresh: async () => {}, ...state }) as unknown as Live<unknown>;

afterEach(() => forgetAll());

const props = { title: 'Lampes Hue', driver: hue, empty: 'Aucune lampe', emptyHint: 'Allume-les', list: list(), reachable: 0, lamps: [] };

describe('LampGroup', () => {
	it('is a region named by its title, with its actions and unfolded panel', async () => {
		await render(LampGroup, { ...props, actions: html('<button>Rechercher</button>'), children: html('<p>Panneau</p>') });
		await expect.element(page.getByRole('region', { name: 'Lampes Hue' })).toBeVisible();
		await expect.element(page.getByRole('heading', { level: 2, name: 'Lampes Hue' })).toBeVisible();
		await expect.element(page.getByRole('button', { name: 'Rechercher' })).toBeVisible();
		await expect.element(page.getByText('Panneau')).toBeVisible();
	});

	it('says it is loading, without announcing it (§ 4)', async () => {
		await render(LampGroup, { ...props, list: list({ loading: true }) });
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
		await expect.element(page.getByRole('status')).not.toBeInTheDocument();
	});

	it('a list that cannot be read says so, never « no lamp »', async () => {
		await render(LampGroup, { ...props, list: list({ failed: true, error: new Error('Bluetooth down') }) });
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByText('Bluetooth down')).toBeVisible();
		await expect.element(page.getByText('Aucune lampe')).not.toBeInTheDocument();
	});

	it('says why there is no lamp, and no count', async () => {
		await render(LampGroup, props);
		await expect.element(page.getByText('Aucune lampe')).toBeVisible();
		await expect.element(page.getByText('Allume-les')).toBeVisible();
		await expect.element(page.getByText(m.lamps_reachable_of({ count: 0, total: 0 }))).not.toBeInTheDocument();
	});

	it('one tile per lamp, and « 1 joignable sur 2 »', async () => {
		const lamps = [fromHue(hueLamp()), fromHue(hueLamp({ id: 'hue-2', name: 'Chevet', connected: false }))];
		await render(LampGroup, { ...props, lamps, reachable: 1 });
		await expect.element(page.getByText(m.lamps_reachable_of({ count: 1, total: 2 }))).toBeVisible();
		await expect.element(page.getByRole('article', { name: 'Lampe du salon' })).toBeVisible();
		await expect.element(page.getByRole('article', { name: 'Chevet' })).toBeVisible();
	});
});
