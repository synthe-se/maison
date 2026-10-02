import { afterEach, describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';
import { api } from '#lib/api.ts';
import { forgetAll, live } from '#lib/live.svelte.ts';
import { json, scriptFetch, sentBody } from '#lib/test/fetch.ts';
import { session } from './session.svelte.ts';

const user = { id: '1', username: 'leonard', role: 'admin' };

/** The live entry at `key`, as a component would get it. */
function liveEntry(key: string) {
	let entry!: ReturnType<typeof live<number>>;
	const destroy = $effect.root(() => {
		entry = live(key, async () => 1);
	});
	flushSync();
	destroy();
	return entry;
}

describe('session', () => {
	afterEach(() => {
		session.user = null;
		session.status = 'loading';
		forgetAll();
	});

	it('is signed in when the cookie is still good', async () => {
		scriptFetch(json({ success: true, user }));
		await session.verify();
		expect(session.status).toBe('signed_in');
		expect(session.user).toEqual(user);
	});

	it('is signed out when the server refuses the session (401, even after a refresh)', async () => {
		scriptFetch(json({ error: 'no session' }, 401), json({}, 401));
		await session.verify();
		expect(session.status).toBe('signed_out');
		expect(session.user).toBeNull();
	});

	it('is signed out when the server answers without a user', async () => {
		scriptFetch(json({ success: false }));
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

	it('signs in with the name and password typed', async () => {
		const calls = scriptFetch(json({ success: true, user }));
		await session.signIn('leonard', 'secret');
		expect(calls[0].url).toBe('/api/auth/login');
		expect(sentBody(calls[0])).toEqual({ username: 'leonard', password: 'secret' });
		expect(session.status).toBe('signed_in');
		expect(session.user?.username).toBe('leonard');
	});

	it('a refused sign-in throws the server’s words and changes nothing', async () => {
		session.status = 'signed_out';
		scriptFetch(json({ error: 'Identifiants incorrects' }, 401), json({}, 401));
		await expect(session.signIn('leonard', 'wrong')).rejects.toThrow('Identifiants incorrects');
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
