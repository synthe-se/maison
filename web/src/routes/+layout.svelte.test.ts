import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { api } from '#lib/api.ts';
import { refresh } from '#lib/live.svelte.ts';
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
		session.screen = null;
		ui.setTheme('system');
		localStorage.removeItem('maison-sun');
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
		await expect.element(page.getByText(m.branding_promise())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.reload() })).toBeVisible();
	});

	it('a wall screen opens on the page; removed, it says so, offers to pair again and no passkey', async () => {
		const refused = new Response('{}', { status: 401 });
		const backend = stubApi({
			'POST /auth/verify': refused,
			'POST /auth/refresh': refused,
			'GET /screens/me': { success: true, screen: { name: 'Salon' } },
			'/scenes': refused,
			'POST /screens/pairing': { success: true, code: 'ABCD-EFGH', secret: 's', expiresInMs: 600_000, pollMs: 3_000 }
		});
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: 'Salon' })).toBeVisible();
		await api('/scenes').catch(() => {});
		await expect.element(page.getByRole('heading', { level: 1, name: m.screen_revoked_title() })).toBeVisible();
		await expect.element(page.getByText(m.screen_revoked_body())).toBeVisible();
		expect(page.getByRole('button', { name: m.pk_button() }).query()).toBeNull();
		expect(page.getByRole('navigation', { name: m.nav_label() }).query()).toBeNull();
		await page.getByRole('button', { name: m.screen_pair_retry() }).click();
		await expect.element(page.getByRole('heading', { level: 1, name: m.screen_pair_title() })).toBeVisible();
		await expect.element(page.getByText('ABCD-EFGH')).toBeVisible();
		expect(backend.sent('POST', '/screens/pairing')).toHaveLength(1);
	});

	it('asks to sign in when the session is over', async () => {
		stubApi({
			'POST /auth/verify': new Response('{"error":"expired"}', { status: 401 }),
			'POST /auth/refresh': new Response('{}', { status: 401 })
		});
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: m.pk_signin_title() })).toBeVisible();
		await expect.element(page.getByText('Salon')).not.toBeInTheDocument();
		await expect.element(page.getByRole('navigation', { name: m.nav_label() })).not.toBeInTheDocument();
	});

	it('signed in from the door: the focus goes to the page’s title, not to the void', async () => {
		stubApi({
			'POST /auth/verify': new Response('{"error":"expired"}', { status: 401 }),
			'POST /auth/refresh': new Response('{}', { status: 401 })
		});
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
		stubApi({
			'POST /auth/verify': new Response('{"error":"expired"}', { status: 401 }),
			'POST /auth/refresh': new Response('{}', { status: 401 })
		});
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
		ui.toast('Panne', { warn: true });
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

	/** The house's place, the sun rising next (night) or setting next (day). */
	const sunAt = (night: boolean) => {
		const h = 3_600_000;
		const now = Date.now();
		return {
			success: true,
			place: { name: 'Paris', latitude: 48.85, longitude: 2.35 },
			sun: { sunriseMs: now + (night ? h : 13 * h), sunsetMs: now + (night ? 12 * h : 2 * h), side: night ? 'dark' : 'light' }
		};
	};

	it('a wall screen follows the sun with nothing chosen: dark at night, light by day', async () => {
		const refused = new Response('{}', { status: 401 });
		const backend = stubApi({
			'POST /auth/verify': refused,
			'POST /auth/refresh': refused,
			'GET /screens/me': { success: true, screen: { name: 'Salon' } },
			'GET /matter/place': sunAt(true)
		});
		await show();
		await expect.poll(() => document.documentElement.dataset.theme).toBe('dark');
		// the sun has risen: its source, read again just after, says day
		backend.routes['GET /matter/place'] = sunAt(false);
		await refresh('shutters:place');
		await expect.poll(() => document.documentElement.dataset.theme).toBe('light');
		// a theme chosen on the screen is kept over the sun
		ui.setTheme('dark');
		await expect.poll(() => document.documentElement.dataset.theme).toBe('dark');
	});

	it('signed out, a page following the sun asks for the side alone, and keeps it for the next first paint', async () => {
		const refused = new Response('{}', { status: 401 });
		stubApi({ 'POST /auth/verify': refused, 'POST /auth/refresh': refused, 'GET /sun': { success: true, side: 'dark' } });
		ui.setTheme('sun');
		await show();
		await expect.element(page.getByRole('heading', { level: 1, name: m.pk_signin_title() })).toBeVisible();
		await expect.poll(() => document.documentElement.dataset.theme).toBe('dark');
		const kept = JSON.parse(localStorage.getItem('maison-sun') ?? 'null');
		expect(kept.side).toBe('dark');
		expect(kept.untilMs).toBeGreaterThan(Date.now());
		// back to the system's: the side is forgotten, nothing applied at the next first paint
		ui.setTheme('system');
		await expect.poll(() => localStorage.getItem('maison-sun')).toBeNull();
		await expect.poll(() => document.documentElement.dataset.theme).toBeUndefined();
	});

	it('no daylight flash at night while the session is checked: the side last said stands', async () => {
		localStorage.setItem('maison-sun', JSON.stringify({ side: 'dark', untilMs: Date.now() + 3_600_000 }));
		ui.setTheme('sun');
		stubApi({ 'POST /auth/verify': () => new Promise(() => {}) });
		await show();
		await expect.element(page.getByRole('main')).toHaveTextContent(m.common_loading());
		expect(document.documentElement.dataset.theme).toBe('dark');
		expect(localStorage.getItem('maison-sun')).not.toBeNull();
	});

	it('someone may choose the sun too; with no place set it is the system’s', async () => {
		const backend = stubApi({ ...signedIn, 'GET /matter/place': sunAt(true) });
		await show();
		ui.setTheme('sun');
		await expect.poll(() => document.documentElement.dataset.theme).toBe('dark');
		backend.routes['GET /matter/place'] = { success: true, place: null, sun: null };
		await refresh('shutters:place');
		await expect.poll(() => document.documentElement.dataset.theme).toBeUndefined();
	});
});
