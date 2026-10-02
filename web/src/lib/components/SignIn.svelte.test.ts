import { afterEach, describe, expect, it } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { json, scriptFetch, sentBody, stubFetch } from '#lib/test/fetch.ts';
import SignIn from './SignIn.svelte';

async function fill(username: string, password: string) {
	await page.getByLabelText(m.auth_username()).fill(username);
	await page.getByLabelText(m.auth_password()).fill(password);
}

describe('SignIn', () => {
	afterEach(() => {
		session.status = 'loading';
		session.user = null;
	});

	it('signs in with what was typed', async () => {
		const calls = scriptFetch(json({ success: true, user: { id: '1', username: 'leonard', role: 'admin' } }));
		await render(SignIn);
		await expect.element(page.getByRole('heading', { level: 1, name: m.auth_login() })).toBeVisible();
		await fill('leonard', 'secret');
		await page.getByRole('button', { name: m.auth_login() }).click();
		await expect.poll(() => session.status).toBe('signed_in');
		expect(sentBody(calls[0])).toEqual({ username: 'leonard', password: 'secret' });
	});

	it('submits with Enter', async () => {
		const calls = scriptFetch(json({ success: true, user: { id: '1', username: 'leonard', role: 'admin' } }));
		await render(SignIn);
		await fill('leonard', 'secret');
		await userEvent.keyboard('{Enter}');
		await expect.poll(() => calls.length).toBe(1);
	});

	it('says the server’s refusal, as an alert', async () => {
		scriptFetch(json({ error: 'Identifiants incorrects' }, 401), json({}, 401));
		await render(SignIn);
		await fill('leonard', 'faux');
		await page.getByRole('button', { name: m.auth_login() }).click();
		await expect.element(page.getByRole('alert')).toHaveTextContent('Identifiants incorrects');
		expect(session.status).not.toBe('signed_in');
	});

	it('says « wrong credentials » when the error has no words', async () => {
		scriptFetch(new Error(''));
		await render(SignIn);
		await fill('leonard', 'faux');
		await page.getByRole('button', { name: m.auth_login() }).click();
		await expect.element(page.getByRole('alert')).toHaveTextContent(m.auth_invalid_credentials());
	});

	it('cannot be sent twice while signing in', async () => {
		let answer!: (r: Response) => void;
		stubFetch(() => new Promise<Response>((r) => (answer = r)));
		await render(SignIn);
		await fill('leonard', 'secret');
		await page.getByRole('button', { name: m.auth_login() }).click();
		const busy = page.getByRole('button', { name: m.auth_logging_in() });
		await expect.element(busy).toBeDisabled();
		answer(json({ success: true, user: { id: '1', username: 'leonard', role: 'admin' } }));
		await expect.element(page.getByRole('button', { name: m.auth_login() })).toBeEnabled();
	});
});
