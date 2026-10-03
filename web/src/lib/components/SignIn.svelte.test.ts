import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { json } from '#lib/test/fetch.ts';
import { authenticator, passkeyRoutes } from '#lib/test/passkeys.ts';
import SignIn from './SignIn.svelte';

const button = () => page.getByRole('button', { name: m.pk_button() });

describe('SignIn', () => {
	afterEach(() => {
		session.status = 'loading';
		session.user = null;
	});

	it('one button, no field: the passkey explained in familiar words', async () => {
		await render(SignIn);
		await expect.element(page.getByRole('heading', { level: 1, name: m.pk_signin_title() })).toBeVisible();
		await expect.element(page.getByText(m.pk_signin_intro())).toBeVisible();
		await expect.element(page.getByText(m.pk_handshake())).toBeVisible();
		await expect.element(page.getByText(m.pk_no_key())).toBeVisible();
		await expect.element(page.getByRole('textbox')).not.toBeInTheDocument();
	});

	it('signs in with the passkey the device offers', async () => {
		authenticator();
		const api = stubApi(passkeyRoutes());
		await render(SignIn);
		await button().click();
		await expect.poll(() => session.status).toBe('signed_in');
		expect(api.sent('POST', '/passkeys/login/start')[0].body).toEqual({});
	});

	it('says plainly when the dialog was closed, and the button comes back', async () => {
		authenticator(new DOMException('closed', 'NotAllowedError'));
		stubApi(passkeyRoutes());
		await render(SignIn);
		await button().click();
		await expect.element(page.getByRole('alert')).toHaveTextContent(m.pk_cancelled());
		await expect.element(button()).toBeEnabled();
		expect(session.status).not.toBe('signed_in');
	});

	it('says the server’s refusal in its own words', async () => {
		authenticator();
		stubApi({ ...passkeyRoutes(), 'POST /passkeys/login/finish': json({ error: 'x', code: 'passkey_rejected' }, 400) });
		await render(SignIn);
		await button().click();
		await expect.element(page.getByRole('alert')).toHaveTextContent(m.pk_rejected());
	});

	it('cannot be pressed twice while the device waits', async () => {
		authenticator(new Promise<never>(() => {}));
		stubApi(passkeyRoutes());
		await render(SignIn);
		await button().click();
		await expect.element(page.getByRole('button', { name: m.pk_waiting() })).toBeDisabled();
	});

	it('on an insecure address, says where to go instead of offering a button that cannot work', async () => {
		vi.stubGlobal('isSecureContext', false);
		await render(SignIn);
		await expect.element(page.getByRole('alert')).toHaveTextContent(m.pk_insecure());
		await expect.element(button()).not.toBeInTheDocument();
	});
});
