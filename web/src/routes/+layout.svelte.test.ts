import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { html } from '#lib/test/snippet.ts';
import Layout from './+layout.svelte';

// SvelteKit's navigation hooks and app state, as the layout uses them
const kit = vi.hoisted(() => ({
	after: [] as (() => unknown)[],
	before: [] as ((n: { willUnload: boolean; to: { url: URL } | null }) => void)[],
	updated: { current: false, check: async () => false },
	page: { url: new URL('http://maison.test/'), params: {} }
}));
vi.mock('$app/navigation', () => ({
	afterNavigate: (f: () => unknown) => kit.after.push(f),
	beforeNavigate: (f: (n: { willUnload: boolean; to: { url: URL } | null }) => void) => kit.before.push(f)
}));
vi.mock('$app/state', () => ({ updated: kit.updated, page: kit.page }));

const signedIn = { 'POST /auth/verify': { success: true, user: { id: 'leonard', name: 'léonard', role: 'admin' } } };
const show = () => render(Layout, { children: html('<div><h1 tabindex="-1">Salon</h1></div>') });

describe('layout', () => {
	beforeEach(() => {
		session.status = 'loading';
		session.user = null;
		kit.after = [];
		kit.before = [];
		kit.page.url = new URL('http://maison.test/');
	});
	afterEach(() => {
		session.status = 'loading';
		session.user = null;
		ui.setTheme('system');
	});

	it('says it is loading while the session is checked, with a skip link', async () => {
		stubApi({ 'POST /auth/verify': () => new Promise(() => {}) });
		await show();
		await expect.element(page.getByRole('main')).toHaveTextContent(m.common_loading());
		// the one status region is there from the start, empty: a load is not announced (§ 4)
		await expect.element(page.getByRole('status')).toHaveTextContent('');
		await expect.element(page.getByRole('link', { name: m.skip_to_content() })).toHaveAttribute('href', '#main');
		await expect.element(page.getByRole('main')).toHaveAttribute('id', 'main');
	});

	it('says the server is not answering, with a way to reload', async () => {
		stubApi({ 'POST /auth/verify': new TypeError('Failed to fetch') });
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: m.unreachable_title() })).toBeVisible();
		await expect.element(page.getByText(m.unreachable_body())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.reload() })).toBeVisible();
	});

	it('asks to sign in when the session is over', async () => {
		stubApi({ 'POST /auth/verify': new Response('{"error":"expired"}', { status: 401 }), 'POST /auth/refresh': new Response('{}', { status: 401 }) });
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: m.pk_signin_title() })).toBeVisible();
		await expect.element(page.getByText('Salon')).not.toBeInTheDocument();
		await expect.element(page.getByRole('navigation', { name: m.nav_label() })).not.toBeInTheDocument();
	});

	it('signed in from the door: the focus goes to the page’s title, not to the void', async () => {
		stubApi({ 'POST /auth/verify': new Response('{"error":"expired"}', { status: 401 }), 'POST /auth/refresh': new Response('{}', { status: 401 }) });
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: m.pk_signin_title() })).toBeVisible();
		session.adopt({ id: 'leonard', name: 'Léonard', role: 'admin' });
		await expect.element(page.getByRole('heading', { name: 'Salon' })).toHaveFocus();
	});

	it('the server not answering: the window says so too', async () => {
		stubApi({ 'POST /auth/verify': new Response('<html>', { status: 502 }) });
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: m.unreachable_title() })).toBeVisible();
		await expect.poll(() => document.title).toContain(m.unreachable_title());
	});

	it('opens an invitation signed out: it is how one gets a first passkey', async () => {
		kit.page.url = new URL('http://maison.test/invite/tok');
		stubApi({ 'POST /auth/verify': new Response('{"error":"expired"}', { status: 401 }), 'POST /auth/refresh': new Response('{}', { status: 401 }) });
		await show();
		await expect.element(page.getByRole('heading', { name: 'Salon' })).toBeVisible();
		await expect.element(page.getByRole('heading', { name: m.pk_signin_title() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('navigation', { name: m.nav_label() })).not.toBeInTheDocument();
	});

	it('shows the page under the header once signed in', async () => {
		stubApi(signedIn);
		await show();
		await expect.element(page.getByRole('main').getByRole('heading', { name: 'Salon' })).toBeVisible();
		await expect.element(page.getByRole('navigation', { name: m.nav_label() }).first()).toBeVisible();
	});

	it('moves the focus to the new view’s title after a navigation, not on the first load', async () => {
		stubApi(signedIn);
		await show();
		const title = page.getByRole('heading', { name: 'Salon' });
		await expect.element(title).toBeVisible();
		for (const f of kit.after) await f();
		await expect.element(title).not.toHaveFocus();
		for (const f of kit.after) await f();
		await expect.element(title).toHaveFocus();
	});

	it('leaves a navigation alone while no new version is deployed', async () => {
		stubApi(signedIn);
		await show();
		const before = location.href;
		for (const f of kit.before) f({ willUnload: false, to: { url: new URL('/remote', location.origin) } });
		expect(location.href).toBe(before);
	});

	it('says each outcome in its live regions', async () => {
		stubApi(signedIn);
		await show();
		ui.say('Volet ouvert');
		await expect.element(page.getByText('Volet ouvert')).toBeInTheDocument();
		ui.toast('Panne', true);
		await expect.element(page.getByRole('alert').filter({ hasText: 'Panne' }).first()).toBeInTheDocument();
		ui.toasts = [];
	});

	it('applies the chosen theme, and follows the system otherwise', async () => {
		stubApi(signedIn);
		await show();
		ui.setTheme('dark');
		await expect.poll(() => document.documentElement.dataset.theme).toBe('dark');
		ui.setTheme('system');
		await expect.poll(() => document.documentElement.dataset.theme).toBeUndefined();
	});
});
