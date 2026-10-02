// Who is signed in. The session is an HttpOnly cookie the backend sets; the page only asks
// whether it is still good, and forgets everything on sign-out.

import { ApiError, authApi, onUnauthorized } from '#lib/api.ts';
import { forgetAll } from '#lib/live.svelte.ts';

type User = { id: string; username: string; role: string };

class Session {
	/** `unreachable`: no answer, or the tunnel's 502 while the Pi restarts. */
	status = $state<'loading' | 'signed_in' | 'signed_out' | 'unreachable'>('loading');
	user = $state.raw<User | null>(null);

	constructor() {
		// a request refused after the refresh attempt: the session is over
		onUnauthorized(() => this.#out());
	}

	async verify() {
		try {
			const r = await authApi.verify();
			if (r.success && r.user) {
				this.user = r.user;
				this.status = 'signed_in';
				return;
			}
		} catch (e) {
			// refused: no usable session; anything else: the server is not there to ask
			if (!(e instanceof ApiError) || e.status >= 500) {
				this.status = 'unreachable';
				return;
			}
		}
		this.#out();
	}

	async signIn(username: string, password: string) {
		const r = await authApi.login(username, password);
		if (r.success) {
			this.user = r.user;
			this.status = 'signed_in';
		}
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
		this.status = 'signed_out';
		forgetAll();
	}
}

export const session = new Session();
