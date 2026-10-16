import { afterEach, describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';
import { api } from '#lib/api.ts';
import { live, source } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { json, scriptFetch } from '#lib/test/fetch.ts';
import { alex, authenticator, leonard, passkeyRoutes } from '#lib/test/passkeys.ts';
import { session } from './session.svelte.ts';

const user = leonard;

/** The live entry at `key`, as a component would get it. */
function liveEntry(key: string) {
	let entry!: ReturnType<typeof live<number>>;
	const destroy = $effect.root(() => {
		entry = live(source(key, async () => 1));
	});
	flushSync();
	destroy();
	return entry;
}

describe('session', () => {
	afterEach(() => {
		session.user = null;
		session.status = 'loading';
		session.screen = null;
	});

	/** Nobody signed in: the session refused, its renewal too. */
	const nobody = () => [json({ error: 'no session' }, 401), json({}, 401)];

	it('a wall screen is in by its cookie, nobody signed in; removed, it says so, not signed out', async () => {
		const calls = scriptFetch(...nobody(), json({ success: true, screen: { name: 'Salon' } }));
		await session.verify();
		expect(calls.map((c) => c.url)).toEqual(['/api/auth/verify', '/api/auth/refresh', '/api/screens/me']);
		expect(session.status).toBe('signed_in');
		expect(session.screen).toBe('Salon');
		expect(session.user).toBeNull();
		expect(session.admin).toBe(false);
		// an admin removed it: its next request is refused, its renewal too
		scriptFetch(json({ error: 'removed' }, 401), json({}, 401));
		await expect(api('/scenes')).rejects.toThrow('removed');
		expect(session.status).toBe('revoked');
	});

	it('a screen just paired is in at once, by its name', () => {
		session.becomeScreen('Salon');
		expect(session.status).toBe('signed_in');
		expect(session.screen).toBe('Salon');
	});

	it('is signed in when the cookie is still good', async () => {
		scriptFetch(json({ success: true, user }));
		await session.verify();
		expect(session.status).toBe('signed_in');
		expect(session.user).toEqual(user);
	});

	it('is signed out when the server refuses the session (401, even after a refresh) and it is no screen', async () => {
		scriptFetch(...nobody(), ...nobody());
		await session.verify();
		expect(session.status).toBe('signed_out');
		expect(session.user).toBeNull();
	});

	it('is signed out when the server answers without a user', async () => {
		scriptFetch(json({ success: false }), ...nobody());
		await session.verify();
		expect(session.status).toBe('signed_out');
	});

	it('is unreachable, not signed out, when nothing answers', async () => {
		scriptFetch(new TypeError('Failed to fetch'));
		await session.verify();
		expect(session.status).toBe('unreachable');
	});

	it('is unreachable on the tunnel’s 502 while the Pi restarts', async () => {
		scriptFetch(new Response('<html>Bad gateway</html>', { status: 502 }));
		await session.verify();
		expect(session.status).toBe('unreachable');
	});

	it('signs in with a passkey: no name, no password', async () => {
		authenticator();
		const backend = stubApi(passkeyRoutes());
		await session.signIn();
		expect(backend.sent('POST', '/passkeys/login/finish')).toHaveLength(1);
		expect(session.status).toBe('signed_in');
		expect(session.user?.name).toBe('Léonard');
		expect(session.admin).toBe(true);
	});

	it('a closed passkey dialog throws and changes nothing', async () => {
		session.status = 'signed_out';
		authenticator(null);
		stubApi(passkeyRoutes());
		await expect(session.signIn()).rejects.toMatchObject({ code: 'cancelled' });
		expect(session.status).toBe('signed_out');
	});

	it('adopts the person an invitation has just let in; a member is no admin', () => {
		session.adopt(alex);
		expect(session.status).toBe('signed_in');
		expect(session.admin).toBe(false);
		session.adopt(undefined);
		expect(session.user).toEqual(alex);
	});

	it('signs out everywhere: the server ends every session, this one forgotten too', async () => {
		session.adopt(user);
		const calls = scriptFetch(json({ success: true }));
		await session.signOutEverywhere();
		expect(calls[0].url).toBe('/api/auth/logout-everywhere');
		expect(session.status).toBe('signed_out');
	});

	it('signs out on the server and forgets every live value', async () => {
		session.status = 'signed_in';
		session.user = user;
		const before = liveEntry('lamps');
		const calls = scriptFetch(json({ success: true }));
		await session.signOut();
		expect(calls[0].url).toBe('/api/auth/logout');
		expect(session.status).toBe('signed_out');
		expect(session.user).toBeNull();
		expect(liveEntry('lamps')).not.toBe(before);
	});

	it('signs out locally even when the server does not answer', async () => {
		session.status = 'signed_in';
		scriptFetch(new TypeError('Failed to fetch'));
		await session.signOut();
		expect(session.status).toBe('signed_out');
	});

	it('ends when any request stays refused after the refresh', async () => {
		session.status = 'signed_in';
		session.user = user;
		scriptFetch(json({ error: 'expired' }, 401), json({}, 401));
		await expect(api('/lamps')).rejects.toThrow('expired');
		expect(session.status).toBe('signed_out');
		expect(session.user).toBeNull();
	});
});
