import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { page as screen } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { locale, localeName, switchLocale } from '#lib/i18n.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { json, scriptFetch } from '#lib/test/fetch.ts';
import Header from './Header.svelte';

// the current route, as SvelteKit would say it
const route = vi.hoisted(() => ({ url: new URL('http://maison.test/') }));
vi.mock('$app/state', () => ({ page: route }));

const at = (path: string) => (route.url = new URL(path, 'http://maison.test'));

/** The links of the header's navigation (the bottom bar repeats them under 600 px). */
const nav = () => screen.getByRole('navigation', { name: m.nav_label() }).first();

describe('Header', () => {
	const startLocale = locale();
	beforeEach(() => {
		session.user = { id: '1', username: 'léonard', role: 'admin' };
		at('/');
	});
	afterEach(() => {
		session.user = null;
		session.status = 'loading';
		ui.setTheme('system');
		switchLocale(startLocale);
		localStorage.removeItem('maison-locale');
	});

	it('marks the current destination in the navigation', async () => {
		at('/remote/tv');
		await render(Header);
		await expect.element(nav().getByRole('link', { name: m.nav_remote() })).toHaveAttribute('aria-current', 'page');
		await expect.element(nav().getByRole('link', { name: m.nav_home() })).not.toHaveAttribute('aria-current');
		await expect.element(nav().getByRole('link', { name: m.nav_tempo() })).toHaveAttribute('href', '/tempo-predictions');
	});

	it('a device page belongs to the home destination', async () => {
		at('/hue-lamp/3');
		await render(Header);
		await expect.element(nav().getByRole('link', { name: m.nav_home() })).toHaveAttribute('aria-current', 'page');
	});

	it('opens the session panel from the person’s name', async () => {
		await render(Header);
		await screen.getByRole('button', { name: m.session_menu({ name: 'léonard' }) }).click();
		const panel = screen.getByRole('dialog', { name: 'léonard' });
		await expect.element(panel).toBeVisible();
		await expect.element(panel.getByRole('group', { name: m.language_label() })).toBeVisible();
		await expect.element(panel.getByRole('group', { name: m.theme_label() })).toBeVisible();
	});

	it('switches the language; the current one is pressed', async () => {
		switchLocale('fr');
		await render(Header);
		await screen.getByRole('button', { name: m.session_menu({ name: 'léonard' }) }).click();
		await expect.element(screen.getByRole('button', { name: localeName('fr') })).toHaveAttribute('aria-pressed', 'true');
		const english = screen.getByRole('button', { name: localeName('en') });
		await expect.element(english).toHaveAttribute('aria-pressed', 'false');
		await english.click();
		expect(locale()).toBe('en');
		expect(document.documentElement.lang).toBe('en');
		await expect.element(english).toHaveAttribute('aria-pressed', 'true');
		await expect.element(screen.getByRole('group', { name: 'Language' })).toBeVisible();
	});

	it('switches the theme; the current one is pressed', async () => {
		await render(Header);
		await screen.getByRole('button', { name: m.session_menu({ name: 'léonard' }) }).click();
		await expect.element(screen.getByRole('button', { name: m.theme_system() })).toHaveAttribute('aria-pressed', 'true');
		const dark = screen.getByRole('button', { name: m.theme_dark() });
		await dark.click();
		expect(ui.theme).toBe('dark');
		await expect.element(dark).toHaveAttribute('aria-pressed', 'true');
		await expect.element(screen.getByRole('button', { name: m.theme_system() })).toHaveAttribute('aria-pressed', 'false');
	});

	it('signs out', async () => {
		session.status = 'signed_in';
		const calls = scriptFetch(json({ success: true }));
		await render(Header);
		await screen.getByRole('button', { name: m.session_menu({ name: 'léonard' }) }).click();
		await screen.getByRole('button', { name: m.auth_logout() }).click();
		await expect.poll(() => session.status).toBe('signed_out');
		expect(calls[0].url).toBe('/api/auth/logout');
	});
});
