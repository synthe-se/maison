import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
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
const people = [
	{ id: 'leonard', name: 'Léonard', role: 'admin', passkeys: 2 },
	{ id: 'francia', name: 'Francia', role: 'member', passkeys: 1 }
];

const here = {
	id: 's-here',
	createdMs: Date.UTC(2026, 9, 1, 8),
	lastSeenMs: Date.now(),
	ip: '203.0.113.7',
	userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Safari/605.1.15',
	device: 'Mac',
	current: true
};
const there = {
	id: 's-there',
	createdMs: Date.UTC(2026, 8, 20, 8),
	lastSeenMs: Date.UTC(2026, 8, 28, 19),
	ip: '198.51.100.4',
	userAgent: 'curl/8',
	device: '',
	current: false
};
const sessionsRegion = () => page.getByRole('region', { name: m.sessions_title() });

const salon = { id: 's1', name: 'Salon', createdMs: Date.UTC(2026, 9, 16), lastUsedMs: null };

/** The account's backend: my passkeys, the pending invitations, the screens, and what changes them. */
function site(more: Record<string, unknown> = {}) {
	return stubApi({
		'GET /passkeys': { success: true, passkeys: [iphone, mac] },
		'GET /invites': { success: true, invites: [pending] },
		'GET /people': { success: true, people },
		'GET /auth/sessions': { success: true, sessions: [here, there] },
		'GET /screens': { success: true, screens: [salon] },
		...passkeyRoutes(),
		...more
	});
}
const screensRegion = () => page.getByRole('region', { name: m.screens_title() });
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

	it('renames a passkey in place; the focus goes back to its Rename button', async () => {
		const api = site({ 'PATCH /passkeys/k1': passkey({ name: 'Mon iPhone' }) });
		const say = vi.spyOn(ui, 'say');
		await render(AccountPage);
		await rows()
			.nth(0)
			.getByRole('button', { name: m.keys_rename_of({ name: 'iPhone' }) })
			.click();
		const field = page.getByLabelText(m.keys_rename_label());
		await expect.element(field).toHaveFocus();
		await field.fill('Mon iPhone');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.element(field).not.toBeInTheDocument();
		expect(api.sent('PATCH', '/passkeys/k1')[0].body).toEqual({ name: 'Mon iPhone' });
		expect(say).toHaveBeenCalledWith(m.common_renamed({ name: 'Mon iPhone' }));
		await expect.element(page.getByRole('button', { name: m.keys_rename_of({ name: 'iPhone' }) })).toHaveFocus();
	});

	it('Escape gives up a rename, and the focus goes back to its button', async () => {
		const api = site();
		await render(AccountPage);
		await rows()
			.nth(0)
			.getByRole('button', { name: m.keys_rename_of({ name: 'iPhone' }) })
			.click();
		await page.getByLabelText(m.keys_rename_label()).fill('X');
		await userEvent.keyboard('{Escape}');
		await expect.element(page.getByLabelText(m.keys_rename_label())).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.keys_rename_of({ name: 'iPhone' }) })).toHaveFocus();
		expect(api.sent('PATCH', '/passkeys/k1')).toEqual([]);
	});

	it('removes a passkey; the last one is refused, said in words', async () => {
		const api = site({
			'DELETE /passkeys/k2': json({ error: 'x', code: 'last_passkey' }, 409),
			'DELETE /passkeys/k1': new Response(null, { status: 204 })
		});
		await render(AccountPage);
		// asked first, with the precise verb and « Garder »
		await page.getByRole('button', { name: m.keys_remove_label({ name: 'iPhone' }) }).click();
		const dialog = page.getByRole('alertdialog', { name: m.keys_remove_title({ name: 'iPhone' }) });
		await expect.element(dialog.getByRole('button', { name: m.device_keep() })).toBeVisible();
		await dialog.getByRole('button', { name: m.keys_remove_action() }).click();
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.keys_removed());
		// its row is gone: the focus is on the list's title, not lost
		await expect.element(page.getByRole('heading', { name: m.keys_title() })).toHaveFocus();
		await page.getByRole('button', { name: m.keys_remove_label({ name: m.keys_unnamed() }) }).click();
		await page.getByRole('alertdialog').getByRole('button', { name: m.keys_remove_action() }).click();
		await expect.poll(() => ui.toasts.some((t) => t.warn && t.text.includes(m.code_last_passkey()))).toBe(true);
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

	it('an old sign-in: asks the passkey again, says why, then adds it', async () => {
		authenticator();
		let asked = false;
		const say = vi.spyOn(ui, 'say');
		const api = site({
			'POST /passkeys/register/start': () => {
				if (asked) return passkeyRoutes()['POST /passkeys/register/start'];
				asked = true;
				return json({ error: 'again', code: 'reauth_needed' }, 403);
			}
		});
		await render(AccountPage);
		await page.getByRole('button', { name: m.keys_add() }).click();
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.keys_added());
		expect(say).toHaveBeenCalledWith(m.reauth_ask());
		expect(api.sent('POST', '/passkeys/login/finish')).toHaveLength(1);
	});

	it('signs out everywhere', async () => {
		const api = site({ 'POST /auth/logout-everywhere': { success: true } });
		await render(AccountPage);
		await page.getByRole('button', { name: m.everywhere_button() }).click();
		const dialog = page.getByRole('alertdialog', { name: m.everywhere_confirm_title() });
		await dialog.getByRole('button', { name: m.everywhere_button() }).click();
		await expect.poll(() => session.status).toBe('signed_out');
		expect(api.sent('POST', '/auth/logout-everywhere')).toHaveLength(1);
	});

	it('an admin invites someone: a link to copy, and the pending ones to cancel', async () => {
		const url = 'https://maison.example.com/invite/abc';
		const api = site({
			'POST /invites': { id: 'i2', url, expiresMs: Date.UTC(2026, 9, 10) },
			'DELETE /invites/i1': new Response(null, { status: 204 })
		});
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
		const link = page.getByLabelText(m.invites_link({ name: 'Francia' }));
		await expect.element(link).toHaveValue(url);
		// the outcome has the focus, ready to copy
		await expect.element(link).toHaveFocus();
		await page.getByRole('button', { name: m.invites_copy() }).click();
		expect(write).toHaveBeenCalledWith(url);
		await page.getByRole('button', { name: m.invites_revoke_label({ name: 'Alex' }) }).click();
		await page
			.getByRole('alertdialog', { name: m.invites_revoke_title({ name: 'Alex' }) })
			.getByRole('button', { name: m.invites_revoke() })
			.click();
		await expect.poll(() => api.sent('DELETE', '/invites/i1')).toHaveLength(1);
		await expect.element(page.getByRole('heading', { name: m.invites_title(), level: 2 })).toHaveFocus();
	});

	it('« Créer le lien » with no name says what is missing, under the field', async () => {
		const api = site();
		await render(AccountPage);
		await page.getByRole('button', { name: m.invites_title() }).click();
		const sheet = page.getByRole('dialog', { name: m.invites_title() });
		await sheet.getByRole('button', { name: m.invites_create() }).click();
		const field = sheet.getByLabelText(m.invites_name());
		await expect.element(field).toHaveAccessibleDescription(m.invites_name_missing());
		await expect.element(field).toHaveFocus();
		expect(api.sent('POST', '/invites')).toEqual([]);
	});

	it('a name already someone’s: offers their access back, sent for that person', async () => {
		const url = 'https://maison.example.com/invite/xyz';
		const api = site({
			'POST /invites': ({ body }: { body: { person?: string } }) =>
				body.person
					? { id: 'i3', url, expiresMs: Date.UTC(2026, 9, 10) }
					: json({ error: 'exists', code: 'person_exists', detail: { person: { id: 'francia-b', name: 'Francia' } } }, 409)
		});
		await render(AccountPage);
		await page.getByRole('button', { name: m.invites_title() }).click();
		const sheet = page.getByRole('dialog', { name: m.invites_title() });
		await sheet.getByLabelText(m.invites_name()).fill('Francia');
		await sheet.getByRole('button', { name: m.invites_create() }).click();
		await expect.element(sheet.getByLabelText(m.invites_name())).toHaveAccessibleDescription(m.invites_person_exists({ name: 'Francia' }));
		await sheet.getByRole('button', { name: m.invites_access_back({ name: 'Francia' }) }).click();
		await expect
			.poll(() => api.sent('POST', '/invites').map((c) => c.body))
			.toEqual([
				{ name: 'Francia', admin: false },
				{ name: 'Francia', admin: false, person: 'francia-b' }
			]);
		await expect.element(page.getByLabelText(m.invites_link({ name: 'Francia' }))).toHaveValue(url);
	});

	it('an admin sees who may come in, removes someone after asking, never themself', async () => {
		const api = site({ 'DELETE /people/francia': new Response(null, { status: 204 }) });
		await render(AccountPage);
		const region = page.getByRole('region', { name: m.people_title() });
		await expect.element(region.getByText(m.people_passkeys({ count: 2 }))).toBeVisible();
		await expect.element(region.getByRole('button', { name: m.common_remove_named({ name: 'Léonard' }) })).not.toBeInTheDocument();
		await expect.element(region.getByText(m.people_you())).toBeVisible();
		await region.getByRole('button', { name: m.common_remove_named({ name: 'Francia' }) }).click();
		await page
			.getByRole('alertdialog', { name: m.common_remove_from_maison({ name: 'Francia' }) })
			.getByRole('button', { name: m.people_remove_action() })
			.click();
		await expect.poll(() => api.sent('DELETE', '/people/francia')).toHaveLength(1);
		await expect.element(page.getByRole('heading', { name: m.people_title() })).toHaveFocus();
	});

	it('a list that cannot be read says so (never an empty list)', async () => {
		site({ 'GET /passkeys': new Response('{"error":"down"}', { status: 500 }) });
		await render(AccountPage);
		await expect.element(page.getByRole('region', { name: m.keys_title() }).getByText(m.load_failed())).toBeVisible();
	});

	it('a sheet with a name typed asks before Escape loses it', async () => {
		site();
		await render(AccountPage);
		await page.getByRole('button', { name: m.invites_title() }).click();
		await page.getByRole('dialog', { name: m.invites_title() }).getByLabelText(m.invites_name()).fill('Zoé');
		await userEvent.keyboard('{Escape}');
		await expect.element(page.getByRole('alertdialog', { name: m.sheet_discard_title() })).toBeVisible();
	});

	it('a member manages their passkeys but sees no invitations', async () => {
		session.adopt(alex);
		const api = site();
		await render(AccountPage);
		await expect.element(rows()).toHaveLength(2);
		await expect.element(page.getByRole('heading', { name: m.invites_title() })).not.toBeInTheDocument();
		expect(api.sent('GET', '/invites')).toEqual([]);
		expect(api.sent('GET', '/people')).toEqual([]);
		await expect.element(page.getByRole('heading', { name: m.people_title() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('heading', { name: m.screens_title() })).not.toBeInTheDocument();
		expect(api.sent('GET', '/screens')).toEqual([]);
	});

	it('lists the wall screens: name, when paired, when last seen', async () => {
		site();
		await render(AccountPage);
		const row = screensRegion().getByRole('listitem');
		await expect.element(row).toHaveLength(1);
		await expect.element(row.getByText('Salon')).toBeVisible();
		await expect.element(row.getByText(new RegExp(m.screens_never_used()))).toBeVisible();
	});

	it('pairs a screen from the code it shows; the list says it, the focus back on its title', async () => {
		const api = site({ 'POST /screens': { success: true, screen: { name: 'Cuisine' } } });
		await render(AccountPage);
		await screensRegion().getByRole('button', { name: m.screens_pair() }).click();
		const sheet = page.getByRole('dialog', { name: m.screens_pair() });
		await sheet.getByLabelText(m.screens_code()).fill('abcd-efgh');
		await sheet.getByLabelText(m.screens_name()).fill('Cuisine');
		await sheet.getByRole('button', { name: m.screens_pair() }).click();
		await expect.poll(() => api.sent('POST', '/screens')[0]?.body).toEqual({ code: 'abcd-efgh', name: 'Cuisine' });
		await expect.element(sheet).not.toBeInTheDocument();
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.screens_paired({ name: 'Cuisine' }));
		await expect.poll(() => api.sent('GET', '/screens').length).toBe(2);
		await expect.element(page.getByRole('heading', { name: m.screens_title() })).toHaveFocus();
	});

	it('a code from another network is said under the code; a name taken under the name', async () => {
		const refusal = (code: string, status: number) => new Response(JSON.stringify({ success: false, code }), { status });
		const api = site({ 'POST /screens': refusal('pairing_elsewhere', 403) });
		await render(AccountPage);
		await screensRegion().getByRole('button', { name: m.screens_pair() }).click();
		const sheet = page.getByRole('dialog', { name: m.screens_pair() });
		const code = sheet.getByLabelText(m.screens_code());
		const name = sheet.getByLabelText(m.screens_name());
		await code.fill('ABCD-EFGH');
		await name.fill('Salon');
		await sheet.getByRole('button', { name: m.screens_pair() }).click();
		await expect.element(code).toHaveAccessibleDescription(m.code_pairing_elsewhere());
		await expect.element(code).toHaveAttribute('aria-invalid', 'true');
		api.routes['POST /screens'] = refusal('screen_exists', 409);
		await sheet.getByRole('button', { name: m.screens_pair() }).click();
		await expect.element(name).toHaveAccessibleDescription(m.code_screen_exists());
		await expect.element(sheet).toBeVisible();
	});

	it('asks for both fields before asking the server', async () => {
		const api = site();
		await render(AccountPage);
		await screensRegion().getByRole('button', { name: m.screens_pair() }).click();
		const sheet = page.getByRole('dialog', { name: m.screens_pair() });
		await sheet.getByRole('button', { name: m.screens_pair() }).click();
		await expect.element(sheet.getByLabelText(m.screens_code())).toHaveAccessibleDescription(m.screens_code_missing());
		await expect.element(sheet.getByLabelText(m.screens_code())).toHaveFocus();
		expect(api.sent('POST', '/screens')).toEqual([]);
	});

	it('removes a screen after asking: it is signed out at once', async () => {
		const api = site({ 'DELETE /screens/s1': {} });
		await render(AccountPage);
		await screensRegion()
			.getByRole('button', { name: m.common_remove_named({ name: 'Salon' }) })
			.click();
		const confirm = page.getByRole('alertdialog', { name: m.screens_remove_title({ name: 'Salon' }) });
		await expect.element(confirm.getByText(m.screens_remove_description())).toBeVisible();
		await confirm.getByRole('button', { name: m.screens_remove_action() }).click();
		await expect.poll(() => api.sent('DELETE', '/screens/s1')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.screens_removed({ name: 'Salon' }));
	});

	it('says when nobody is waiting for an invitation', async () => {
		site({ 'GET /invites': { success: true, invites: [] } });
		await render(AccountPage);
		await expect.element(page.getByText(m.invites_none())).toBeVisible();
	});

	it('lists my sessions: the device and browser in words, when opened and seen, from where; this one marked', async () => {
		site();
		await render(AccountPage);
		const listed = sessionsRegion().getByRole('listitem');
		await expect.element(listed).toHaveLength(2);
		await expect.element(listed.nth(0).getByText('Mac · Safari')).toBeVisible();
		await expect.element(listed.nth(0).getByText(m.sessions_this_device())).toBeVisible();
		await expect.element(listed.nth(0).getByText(/203\.0\.113\.7/)).toBeVisible();
		await expect.element(listed.nth(1).getByText(m.sessions_unknown_device())).toBeVisible();
		await expect.element(listed.nth(1).getByText(m.sessions_this_device())).not.toBeInTheDocument();
		await expect.element(listed.nth(1).getByText(new RegExp(m.sessions_since({ when: '.+' })))).toBeVisible();
	});

	it('ends another session at once; the list is read again, the focus on its title', async () => {
		const api = site({ 'DELETE /auth/sessions/s-there': { success: true, message: 'Session ended' } });
		await render(AccountPage);
		await sessionsRegion()
			.getByRole('button', { name: m.sessions_end_label({ device: m.sessions_unknown_device() }) })
			.click();
		await expect.poll(() => api.sent('DELETE', '/auth/sessions/s-there')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.sessions_ended());
		await expect.poll(() => api.sent('GET', '/auth/sessions').length).toBe(2);
		await expect.element(page.getByRole('heading', { name: m.sessions_title() })).toHaveFocus();
		expect(session.status).toBe('signed_in');
	});

	it('ending this device’s session asks first, then signs out here', async () => {
		const api = site({ 'DELETE /auth/sessions/s-here': { success: true, message: 'Session ended' } });
		await render(AccountPage);
		await sessionsRegion()
			.getByRole('button', { name: m.sessions_end_label({ device: 'Mac · Safari' }) })
			.click();
		const dialog = page.getByRole('alertdialog', { name: m.sessions_end_current_title() });
		await expect.element(dialog.getByText(m.sessions_end_current_description())).toBeVisible();
		expect(api.sent('DELETE', '/auth/sessions/s-here')).toEqual([]);
		await dialog.getByRole('button', { name: m.sessions_end_current_action() }).click();
		await expect.poll(() => session.status).toBe('signed_out');
		expect(api.sent('DELETE', '/auth/sessions/s-here')).toHaveLength(1);
	});

	it('an admin signs someone out everywhere after asking; they stay listed', async () => {
		const api = site({ 'POST /people/francia/sessions/end': { success: true, message: 'Sessions ended' } });
		await render(AccountPage);
		const region = page.getByRole('region', { name: m.people_title() });
		await expect.element(region.getByRole('button', { name: m.people_end_sessions_label({ name: 'Léonard' }) })).not.toBeInTheDocument();
		await region.getByRole('button', { name: m.people_end_sessions_label({ name: 'Francia' }) }).click();
		const dialog = page.getByRole('alertdialog', { name: m.people_end_sessions_title({ name: 'Francia' }) });
		await expect.element(dialog.getByText(m.people_end_sessions_description())).toBeVisible();
		await dialog.getByRole('button', { name: m.people_end_sessions() }).click();
		await expect.poll(() => api.sent('POST', '/people/francia/sessions/end')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.people_sessions_ended({ name: 'Francia' }));
		await expect.element(region.getByText('Francia')).toBeVisible();
	});

	it('removing someone after an old sign-in: the passkey asked, said why, then removed', async () => {
		authenticator();
		const say = vi.spyOn(ui, 'say');
		const api = site({
			'DELETE /people/francia': () =>
				api.sent('POST', '/passkeys/login/finish').length
					? new Response(null, { status: 204 })
					: json({ error: 'again', code: 'reauth_needed' }, 403)
		});
		await render(AccountPage);
		await page.getByRole('button', { name: m.common_remove_named({ name: 'Francia' }) }).click();
		await page
			.getByRole('alertdialog', { name: m.common_remove_from_maison({ name: 'Francia' }) })
			.getByRole('button', { name: m.people_remove_action() })
			.click();
		await expect.poll(() => api.sent('DELETE', '/people/francia')).toHaveLength(2);
		expect(say).toHaveBeenCalledWith(m.reauth_ask());
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.people_removed({ name: 'Francia' }));
	});
});
