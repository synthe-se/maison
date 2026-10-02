import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { json } from '#lib/test/fetch.ts';
import { authenticator, leonard, passkeyRoutes } from '#lib/test/passkeys.ts';
import InvitePage from './+page.svelte';

const kit = vi.hoisted(() => ({ goto: vi.fn(async () => {}), page: { params: { token: 'tok' } } }));
vi.mock('$app/navigation', () => ({ goto: kit.goto }));
vi.mock('$app/state', () => ({ page: kit.page }));

const site = (more: Record<string, unknown> = {}) => stubApi({ 'GET /invites/tok': { name: 'Léonard' }, ...passkeyRoutes(), ...more });
const create = () => page.getByRole('button', { name: m.invite_create() });

describe('invitation page', () => {
	afterEach(() => {
		session.status = 'loading';
		session.user = null;
	});

	it('greets the person by name and says what a passkey is, before the system dialog', async () => {
		site();
		await render(InvitePage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.invite_title({ name: 'Léonard' }) })).toBeVisible();
		await expect.element(page.getByText(m.invite_why_nopassword())).toBeVisible();
		await expect.element(page.getByText(m.invite_why_devices())).toBeVisible();
		await expect.element(page.getByText(m.pk_handshake())).toBeVisible();
		await expect.poll(() => document.title).toBe(`${m.invite_title({ name: 'Léonard' })} · ${m.branding_name()}`);
	});

	it('creates the passkey with the link, confirms, then goes in', async () => {
		authenticator();
		const api = site();
		await render(InvitePage);
		await create().click();
		await expect.element(page.getByRole('heading', { level: 1, name: m.invite_done_title() })).toBeVisible();
		expect(api.sent('POST', '/passkeys/register/start')[0].body).toEqual({ invite: 'tok' });
		expect(session.status).not.toBe('signed_in');
		await page.getByRole('button', { name: m.invite_enter() }).click();
		await expect.poll(() => session.user).toEqual(leonard);
		expect(kit.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('a spent or unknown link says so, and offers nothing to press', async () => {
		site({ 'GET /invites/tok': json({ error: 'x', code: 'invite_invalid' }, 404) });
		await render(InvitePage);
		await expect.element(page.getByRole('alert')).toHaveTextContent(m.pk_invite_invalid());
		await expect.element(page.getByRole('button')).not.toBeInTheDocument();
	});

	it('a closed dialog is said, and the button stays', async () => {
		authenticator(null);
		site();
		await render(InvitePage);
		await create().click();
		await expect.element(page.getByRole('alert')).toHaveTextContent(m.pk_cancelled());
		await expect.element(create()).toBeEnabled();
	});

	it('says « checking » while the link is looked up', async () => {
		site({ 'GET /invites/tok': () => new Promise(() => {}) });
		await render(InvitePage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.invite_checking() })).toBeVisible();
	});

	it('on an insecure address, says where to go', async () => {
		vi.stubGlobal('isSecureContext', false);
		site();
		await render(InvitePage);
		await expect.element(page.getByRole('alert').filter({ hasText: m.pk_insecure() })).toBeVisible();
		await expect.element(create()).not.toBeInTheDocument();
	});
});
