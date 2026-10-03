import { afterEach, describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll, live } from '#lib/live.svelte.ts';
import { html } from '#lib/test/snippet.ts';
import Loaded from './Loaded.svelte';

/** A live value driven by the test. */
function value(key: string, fetch: () => Promise<unknown>) {
	let v!: ReturnType<typeof live<unknown>>;
	$effect.root(() => {
		v = live(key, fetch);
	});
	flushSync();
	return v;
}

const content = html('<p>Contenu</p>');

describe('Loaded', () => {
	afterEach(() => forgetAll());

	it('the first load: a line, or skeletons of the tiles’ height; never a live region', async () => {
		await render(Loaded, { value: value('a', () => new Promise(() => {})), skeletons: 2, children: content });
		expect(document.querySelectorAll('.skeleton')).toHaveLength(2);
		await expect.element(page.getByRole('status')).not.toBeInTheDocument();
	});

	it('nothing known and a failure: said as such, with the reason and a retry; the retry brings the content', async () => {
		let fail = true;
		const v = value('b', async () => {
			if (fail) throw new Error('Maison est loin');
			return 1;
		});
		await render(Loaded, { value: v, children: content });
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByText('Maison est loin')).toBeVisible();
		fail = false;
		await page.getByRole('button', { name: m.common_retry() }).click();
		await expect.element(page.getByText('Contenu')).toBeVisible();
	});

	it('loaded and empty: why, and what to do', async () => {
		await render(Loaded, { value: value('c', async () => []), empty: true, emptyText: 'Aucun volet', emptyHint: 'Ajoute-le', children: content });
		await expect.element(page.getByText('Aucun volet')).toBeVisible();
		await expect.element(page.getByText('Ajoute-le')).toBeVisible();
		await expect.element(page.getByText('Contenu')).not.toBeInTheDocument();
	});
});
