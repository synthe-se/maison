// What went wrong, said plainly and with what to do, once for the whole app: a refusal by its
// name (`code`, every one the API answers: codes.ts), no answer by what it means for the person,
// else by its status. The server's own text is never shown (it is English, for logs and curl):
// a refusal without a known code is logged and worded by its status.

import { ApiError, UNREACHABLE } from '#lib/api.ts';
import type { Code } from '#lib/codes.ts';
import { m } from '#lib/paraglide/messages.js';

/** The words of every refusal the API answers: a code without words does not type-check. */
const CODE_WORDS: Record<Code, () => string> = {
	not_signed_in: m.code_not_signed_in,
	forbidden: m.code_forbidden,
	not_for_screens: m.code_not_for_screens,
	bad_name: m.code_bad_name,
	bad_person: m.code_bad_person,
	passkey_off: m.code_passkey_off,
	invite_invalid: m.code_invite_invalid,
	ceremony_expired: m.code_ceremony_expired,
	passkey_rejected: m.code_passkey_rejected,
	unknown_passkey: m.code_unknown_passkey,
	passkey_exists: m.code_passkey_exists,
	last_passkey: m.code_last_passkey,
	not_found: m.code_not_found,
	reauth_needed: m.code_reauth_needed,
	person_exists: m.code_person_exists,
	not_yourself: m.code_not_yourself,
	pairing_unknown: m.code_pairing_unknown,
	pairing_elsewhere: m.code_pairing_elsewhere,
	screen_exists: m.code_screen_exists,
	too_many_attempts: m.code_too_many_attempts,
	nested_scene: m.code_nested_scene,
	too_many_actions: m.code_too_many_actions,
	not_a_device: m.code_not_a_device,
	invalid: m.code_invalid,
	not_configured: m.code_not_configured,
	busy: m.code_busy,
	unsupported: m.code_unsupported,
	device_unreachable: m.code_device_unreachable,
	service_unreachable: m.code_service_unreachable,
	internal: m.code_internal
};

/** The browser's WebAuthn refusals (DOMException names), and a renewal that got no answer. */
const LOCAL_WORDS: Record<string, () => string> = {
	NotAllowedError: m.pk_cancelled,
	AbortError: m.pk_cancelled,
	cancelled: m.pk_cancelled,
	SecurityError: m.pk_insecure,
	InvalidStateError: m.code_passkey_exists,
	[UNREACHABLE]: m.error_network
};

/** The words of `code`, when it is one the app knows. */
export function codeText(code: string | undefined): string | undefined {
	if (!code) return undefined;
	return Object.hasOwn(CODE_WORDS, code) ? CODE_WORDS[code as Code]() : Object.hasOwn(LOCAL_WORDS, code) ? LOCAL_WORDS[code]() : undefined;
}

/** The sentence for `e`, whatever threw it. */
export function errorText(e: unknown): string {
	const code = e instanceof ApiError ? e.code : e instanceof DOMException ? e.name : undefined;
	const known = codeText(code);
	if (known) return known;
	// fetch found no server at all
	if (e instanceof TypeError) return m.error_network();
	// what the server said is for its log, not for the person: kept in the console
	console.warn(e);
	if (e instanceof ApiError) {
		if (e.status === 403) return m.code_forbidden();
		if (e.status === 404) return m.code_not_found();
		// the tunnel's answer while the Pi restarts
		if (e.status === 502 || e.status === 503 || e.status === 504) return m.error_network();
	}
	return m.error_server();
}
