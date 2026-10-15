// This browser as a wall screen (the frame, frame/android): it drives the house with a service
// token (docs/passkeys.md « Service tokens ») instead of a person's session. The token comes once
// in the address's fragment, `#token=maison_…`, which the browser never sends anywhere; it is kept
// here and wiped from the address and the history.

import { PUBLIC_SCREEN_TOKEN_KEY } from '$app/env/public';
import { useToken } from '#lib/api.ts';
import { stored, type Codec } from '#lib/stored.ts';

/** A service token as `maison-backend token create` prints it (backend auth.rs). */
const TOKEN = /^maison_[A-Za-z0-9_-]{43}$/;

const token: Codec<string> = { read: (raw) => (TOKEN.test(raw) ? raw : undefined), write: (v) => v };
const kept = stored<string | null>(PUBLIC_SCREEN_TOKEN_KEY, null, token);

/** Takes a token from the address if it brings one, then says whether this browser is a screen
 * (the requests carry its token from now on). */
export function claimScreen(location: Location = window.location, history: History = window.history): boolean {
	const given = new URLSearchParams(location.hash.slice(1)).get('token');
	let current = kept.get();
	if (given !== null) {
		history.replaceState(history.state, '', location.pathname + location.search);
		if (TOKEN.test(given)) {
			kept.set(given);
			// storage refused (private mode): kept for the visit
			current = given;
		}
	}
	useToken(current);
	return current !== null;
}
