// What went wrong, said plainly and with what to do, once for the whole app: a refusal by its
// name (`code`), no answer by what it means for the person, else the server's own words. Never
// a prefix such as « Erreur. » in front of raw server text.

import { ApiError, UNREACHABLE } from '#lib/api.ts';
import { m } from '#lib/paraglide/messages.js';

const BY_CODE: Record<string, () => string> = {
	// the browser's WebAuthn refusals (DOMException names)
	NotAllowedError: m.pk_cancelled,
	AbortError: m.pk_cancelled,
	cancelled: m.pk_cancelled,
	SecurityError: m.pk_insecure,
	InvalidStateError: m.pk_exists,
	// the server's refusals (backend error codes)
	passkey_exists: m.pk_exists,
	invite_invalid: m.pk_invite_invalid,
	ceremony_expired: m.pk_expired,
	passkey_rejected: m.pk_rejected,
	unknown_passkey: m.pk_unknown,
	last_passkey: m.pk_last,
	too_many_attempts: m.pk_too_many,
	passkey_off: m.pk_off,
	forbidden: m.admin_only,
	not_signed_in: m.error_signed_out,
	bad_name: m.error_bad_name,
	person_exists: m.invites_person_exists_short,
	[UNREACHABLE]: m.error_network
};

/** The sentence for `e`, whatever threw it. */
export function errorText(e: unknown): string {
	const code = e instanceof ApiError ? e.code : e instanceof DOMException ? e.name : undefined;
	const known = code ? BY_CODE[code] : undefined;
	if (known) return known();
	// fetch found no server at all
	if (e instanceof TypeError) return m.error_network();
	if (e instanceof ApiError) {
		if (e.message) return e.message;
		if (e.status === 403) return m.admin_only();
		if (e.status === 404) return m.error_not_found();
		// the tunnel's answer while the Pi restarts
		if (e.status === 502 || e.status === 503 || e.status === 504) return m.error_network();
		return m.error_server();
	}
	return e instanceof Error && e.message ? e.message : m.error_server();
}
