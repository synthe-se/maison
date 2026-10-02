import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { json } from '#lib/test/fetch.ts';
import { alex, authenticator, leonard, passkey, passkeyRoutes } from '#lib/test/passkeys.ts';
import AccountPage from './+page.svelte';

const iphone = passkey();
const mac = passkey({ id: 'k2', name: '', backedUp: false, lastUsedMs: Date.now() });
const pending = { id: 'i1', person: 'alex', name: 'Alex', role: 'member', expiresMs: Date.UTC(2026, 9, 10) };

/** The account's backend: my passkeys, the pending invitations, and what changes them. */
function site(more: Record<string, unknown> = {}) {
	return stubApi({ 'GET /passkeys': [iphone, mac], 'GET /invites': [pending], ...passkeyRoutes(), ...more });
}
const rows = () => page.getByRole('region', { name: m.keys_title() }).getByRole('listitem');

describe('account page', () => {
	beforeEach(() => session.adopt(leonard));
	afterEach(() => {
		session.status = 'loading';
		session.user = null;
		ui.toasts = [];
	});

	it('lists my passkeys: name (or a word for none), synced or not, created and last used', async () => {
		site();
		await render(AccountPage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.account_title() })).toBeVisible();
		await expect.element(page.getByText('Léonard')).toBeVisible();
		await expect.element(rows()).toHaveLength(2);
		const [first, second] = [rows().nth(0), rows().nth(1)];
		await expect.element(first.getByText('iPhone')).toBeVisible();
		await expect.element(first.getByText(m.keys_synced())).toBeVisible();
		await expect.element(first.getByText(new RegExp(m.keys_never_used()))).toBeVisible();
		await expect.element(second.getByText(m.keys_unnamed())).toBeVisible();
		await expect.element(second.getByText(m.keys_synced())).not.toBeInTheDocument();
	});

	it('renames a passkey in place', async () => {
		const api = site({ 'PATCH /passkeys/k1': passkey({ name: 'Mon iPhone' }) });
		await render(AccountPage);
		await rows().nth(0).getByRole('button', { name: m.common_rename() }).click();
		const field = page.getByLabelText(m.keys_rename_label());
		await expect.element(field).toHaveFocus();
		await field.fill('Mon iPhone');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.element(field).not.toBeInTheDocument();
		expect(api.sent('PATCH', '/passkeys/k1')[0].body).toEqual({ name: 'Mon iPhone' });
	});

	it('removes a passkey; the last one is refused, said in words', async () => {
		const api = site({ 'DELETE /passkeys/k2': json({ error: 'x', code: 'last_passkey' }, 409), 'DELETE /passkeys/k1': new Response(null, { status: 204 }) });
		await render(AccountPage);
		await page.getByRole('button', { name: m.keys_remove_label({ name: 'iPhone' }) }).click();
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.keys_removed());
		await page.getByRole('button', { name: m.keys_remove_label({ name: m.keys_unnamed() }) }).click();
		await expect.poll(() => ui.toasts.some((t) => t.warn && t.text.includes(m.pk_last()))).toBe(true);
		expect(api.sent('DELETE', '/passkeys/k2')).toHaveLength(1);
	});

	it('adds a passkey from this device', async () => {
		authenticator();
		const api = site();
		await render(AccountPage);
		await page.getByRole('button', { name: m.keys_add() }).click();
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.keys_added());
		expect(api.sent('POST', '/passkeys/register/start')[0].body).toEqual({});
	});

	it('signs out everywhere', async () => {
		const api = site({ 'POST /auth/logout-everywhere': { success: true } });
		await render(AccountPage);
		await page.getByRole('button', { name: m.everywhere_button() }).click();
		await expect.poll(() => session.status).toBe('signed_out');
		expect(api.sent('POST', '/auth/logout-everywhere')).toHaveLength(1);
	});

	it('an admin invites someone: a link to copy, and the pending ones to cancel', async () => {
		const url = 'https://maison.example.com/invite/abc';
		const api = site({ 'POST /invites': { id: 'i2', url, expiresMs: Date.UTC(2026, 9, 10) }, 'DELETE /invites/i1': new Response(null, { status: 204 }) });
		const write = vi.fn(async () => {});
		vi.spyOn(navigator.clipboard, 'writeText').mockImplementation(write);
		await render(AccountPage);
		await expect.element(page.getByText('Alex')).toBeVisible();
		await page.getByRole('button', { name: m.invites_title() }).click();
		const sheet = page.getByRole('dialog', { name: m.invites_title() });
		await sheet.getByLabelText(m.invites_name()).fill('Francia');
		await sheet.getByRole('switch', { name: m.invites_admin() }).click();
		await sheet.getByRole('button', { name: m.invites_create() }).click();
		await expect.element(sheet).not.toBeInTheDocument();
		expect(api.sent('POST', '/invites')[0].body).toEqual({ name: 'Francia', admin: true });
		await expect.element(page.getByLabelText(m.invites_link({ name: 'Francia' }))).toHaveValue(url);
		await page.getByRole('button', { name: m.invites_copy() }).click();
		expect(write).toHaveBeenCalledWith(url);
		await page.getByRole('button', { name: m.invites_revoke_label({ name: 'Alex' }) }).click();
		await expect.poll(() => api.sent('DELETE', '/invites/i1')).toHaveLength(1);
	});

	it('a member manages their passkeys but sees no invitations', async () => {
		session.adopt(alex);
		const api = site();
		await render(AccountPage);
		await expect.element(rows()).toHaveLength(2);
		await expect.element(page.getByRole('heading', { name: m.invites_title() })).not.toBeInTheDocument();
		expect(api.sent('GET', '/invites')).toEqual([]);
	});

	it('says when nobody is waiting for an invitation', async () => {
		site({ 'GET /invites': [] });
		await render(AccountPage);
		await expect.element(page.getByText(m.invites_none())).toBeVisible();
	});
});
