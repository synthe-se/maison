import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { when } from '#lib/i18n.svelte.ts';
import { live, source } from '#lib/live.svelte.ts';
import { ApiError } from '#lib/api.ts';
import { html } from '#lib/test/snippet.ts';
import Loaded from './Loaded.svelte';

/** A live value driven by the test. */
function value(key: string, fetch: () => Promise<unknown>) {
	let v!: ReturnType<typeof live<unknown>>;
	$effect.root(() => {
		v = live(source(key, fetch));
	});
	flushSync();
	return v;
}

const content = html('<p>Contenu</p>');

describe('Loaded', () => {
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
		await render(Loaded, {
			value: value('c', async () => []),
			empty: true,
			emptyText: 'Aucun volet',
			emptyHint: 'Ajoute-le',
			children: content
		});
		await expect.element(page.getByText('Aucun volet')).toBeVisible();
		await expect.element(page.getByText('Ajoute-le')).toBeVisible();
		await expect.element(page.getByText('Contenu')).not.toBeInTheDocument();
	});

	it('what the server says does not exist (404, `missing`): « not found », nothing to retry', async () => {
		await render(Loaded, {
			value: value('d', () => Promise.reject(new ApiError('', 404, 'not_found'))),
			missing: 'Lampe introuvable',
			children: content
		});
		await expect.element(page.getByText('Lampe introuvable')).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).not.toBeInTheDocument();
	});

	it('a 404 without `missing` is a failure like any other, with its retry', async () => {
		await render(Loaded, { value: value('e', () => Promise.reject(new ApiError('', 404))), children: content });
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
	});

	it('an old value stays, said once with the time it was read and a retry', async () => {
		let fail = false;
		const v = value('f', async () => {
			if (fail) throw new Error('down');
			return 1;
		});
		await render(Loaded, { value: v, children: content });
		await expect.element(page.getByText('Contenu')).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).not.toBeInTheDocument();
		fail = true;
		await v.refresh();
		await expect.element(page.getByText('Contenu')).toBeVisible();
		await expect.element(page.getByText(m.live_stale({ time: when(v.at) }), { exact: false })).toBeVisible();
		fail = false;
		await page.getByRole('button', { name: m.common_retry() }).click();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).not.toBeInTheDocument();
	});
});
