// Who is signed in. The session is an HttpOnly cookie the backend sets (after a passkey,
// passkeys.ts); the page only asks whether it is still good, and forgets everything on sign-out.
// A wall screen is in by its service token (screen.ts): no person, nothing to ask.

import { ApiError, UNREACHABLE, onUnauthorized } from '#lib/api.ts';
import { authApi, type User } from '#lib/passkeys.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import * as passkeys from '#lib/passkeys.ts';
import { claimScreen } from '#lib/screen.ts';

class Session {
	/** `unreachable`: no answer, or the tunnel's 502 while the Pi restarts. `revoked`: a screen
	 * whose token the server refuses. */
	status = $state<'loading' | 'signed_in' | 'signed_out' | 'unreachable' | 'revoked'>('loading');
	/** A wall screen: in by a service token, nobody signed in. */
	screen = false;
	user = $state.raw<User | null>(null);

	constructor() {
		// a request refused after the refresh attempt: the session is over
		onUnauthorized(() => this.#out());
	}

	async verify() {
		this.screen = claimScreen();
		if (this.screen) {
			this.status = 'signed_in';
			return;
		}
		try {
			const r = await authApi.verify();
			if (r.success && r.user) return this.adopt(r.user);
		} catch (e) {
			// refused: no usable session; anything else (no answer, a 5xx, a renewal that got no
			// answer) is the server not being there to ask
			if (!(e instanceof ApiError) || e.status >= 500 || e.code === UNREACHABLE) {
				this.status = 'unreachable';
				return;
			}
		}
		this.#out();
	}

	/** Whether the person may invite others. */
	get admin() {
		return this.user?.role === 'admin';
	}

	/** Signs in with a passkey: no name, no password. Throws what went wrong (errors.ts words it). */
	async signIn() {
		this.adopt((await passkeys.signIn()).user);
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
