// Who is signed in. The session is an HttpOnly cookie the backend sets (after a passkey,
// passkeys.ts); the page only asks whether it is still good, and forgets everything on sign-out.
// A wall screen is in by a cookie of its own, set when an admin paired it (screens.ts): no
// person, its name only.

import { ApiError, UNREACHABLE, onUnauthorized } from '#lib/api.ts';
import { authApi, signIn, type User } from '#lib/passkeys.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { screensApi, takeAddress } from '#lib/screens.ts';

class Session {
	/** `unreachable`: no answer, or the tunnel's 502 while the Pi restarts. `revoked`: a screen
	 * an admin removed while it was on. */
	status = $state<'loading' | 'signed_in' | 'signed_out' | 'unreachable' | 'revoked'>('loading');
	/** A wall screen's name: in by its cookie, nobody signed in. */
	screen = $state<string | null>(null);
	user = $state.raw<User | null>(null);

	constructor() {
		// a request refused after the refresh attempt: the session is over
		onUnauthorized(() => this.#out());
	}

	async verify() {
		takeAddress();
		// someone signed in, else a paired screen, else nobody: a refusal moves on to the next,
		// anything else (no answer, a 5xx, a renewal that got no answer) is the server not being
		// there to ask
		const absent = (e: unknown) => !(e instanceof ApiError) || e.status >= 500 || e.code === UNREACHABLE;
		try {
			const r = await authApi.verify();
			if (r.success && r.user) return this.adopt(r.user);
		} catch (e) {
			if (absent(e)) return void (this.status = 'unreachable');
		}
		try {
			return this.becomeScreen((await screensApi.me()).screen.name);
		} catch (e) {
			if (absent(e)) return void (this.status = 'unreachable');
		}
		this.#out();
	}

	/** In, as the screen `name`: an admin paired it, or its cookie is still good. */
	becomeScreen(name: string) {
		this.screen = name;
		this.status = 'signed_in';
	}

	/** Whether the person may invite others. */
	get admin() {
		return this.user?.role === 'admin';
	}

	/** Signs in with a passkey: no name, no password. Throws what went wrong (errors.ts words it). */
	async signIn() {
		this.adopt((await signIn()).user);
	}

	/** In, as this person: the server has just opened the session (a sign-in, an invitation). */
	adopt(user: User | undefined) {
		if (!user) return;
		this.user = user;
		this.status = 'signed_in';
	}

	/** A lost phone: every session of mine ends, here too. */
	async signOutEverywhere() {
		await authApi.logoutEverywhere();
		this.#out();
	}

	async signOut() {
		try {
			await authApi.logout();
		} catch {
			// the local state is cleared whatever the server says
		}
		this.#out();
	}

	#out() {
		this.user = null;
		this.status = this.screen ? 'revoked' : 'signed_out';
		forgetAll();
	}
}

export const session = new Session();
