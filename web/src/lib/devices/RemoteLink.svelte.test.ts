import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import RemoteLink from './RemoteLink.svelte';

const tile = () => page.getByRole('article', { name: m.remote_stb_name() });

describe('RemoteLink', () => {
	afterEach(() => forgetAll());
	beforeEach(() => session.adopt(leonard));

	it('a member is offered to look, not to configure', async () => {
		session.adopt(alex);
		stubApi({ '/ir/keymap': { success: true, keymap: {} } });
		await render(RemoteLink);
		await expect.element(page.getByRole('link', { name: m.remote_view() })).toHaveAttribute('href', '/remote');
	});

	it('says how many buttons do something, and leads to the configurator', async () => {
		stubApi({ '/ir/keymap': { success: true, keymap: { '116': { actions: [] }, '2': { actions: [] } } } });
		await render(RemoteLink);
		await expect.element(tile()).toMatchTextContent(m.remote_binding_count({ count: 2 }));
		await expect.element(page.getByRole('link', { name: m.remote_configure() })).toHaveAttribute('href', '/remote');
		await expect.element(page.getByRole('link', { name: m.remote_stb_name() })).toHaveAttribute('href', '/remote');
	});

	it('says when no button is configured yet', async () => {
		stubApi({ '/ir/keymap': { success: true, keymap: {} } });
		await render(RemoteLink);
		await expect.element(tile()).toMatchTextContent(m.remote_no_bindings());
	});

	it('says it is loading meanwhile', async () => {
		stubApi({ '/ir/keymap': () => new Promise(() => {}) });
		await render(RemoteLink);
		await expect.element(tile()).toMatchTextContent(m.common_loading());
	});
});
