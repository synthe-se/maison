// A passkey provider for component tests, said once: the browser's authenticator answers
// (or is closed, or refuses) without a dialog, and the backend's ceremony routes answer as
// webauthn-rs does. Options are real WebAuthn JSON, so Chromium's own parsers read them.

import { vi } from 'vitest';
import type { User } from '#lib/api.ts';
import type { PasskeyInfo } from '#lib/passkeys.ts';

export const leonard: User = { id: 'leonard', name: 'Léonard', role: 'admin' };
export const alex: User = { id: 'alex', name: 'Alex', role: 'member' };

const challenge = 'Y2hhbGxlbmdlLWNoYWxsZW5nZQ';
export const requestJson = { challenge, rpId: 'localhost', timeout: 300000, userVerification: 'required', allowCredentials: [] };
export const creationJson = {
	rp: { id: 'localhost', name: 'Maison' },
	user: { id: 'dXNlci1oYW5kbGU', name: 'leonard', displayName: 'Léonard' },
	challenge,
	pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
	timeout: 300000,
	excludeCredentials: [],
	authenticatorSelection: { residentKey: 'required', requireResidentKey: true, userVerification: 'required' },
	attestation: 'none'
};

/** What the authenticator gives back: the JSON the server reads. */
export const credentialJson = { id: 'Y3JlZA', rawId: 'Y3JlZA', type: 'public-key', response: {} };
const credential = { id: credentialJson.id, toJSON: () => credentialJson } as unknown as Credential;

/**
 * The device's authenticator: answers with a credential, or with `outcome` (null: the dialog
 * was closed; an error: what the browser throws). Returns the spies.
 */
export function authenticator(outcome?: null | Error | Promise<never>) {
	const answer = async () => {
		if (outcome === undefined) return credential;
		if (outcome instanceof Error) throw outcome;
		return outcome;
	};
	return {
		get: vi.spyOn(navigator.credentials, 'get').mockImplementation(answer),
		create: vi.spyOn(navigator.credentials, 'create').mockImplementation(answer)
	};
}

export const passkey = (over: Partial<PasskeyInfo> = {}): PasskeyInfo => ({
	id: 'k1',
	name: 'iPhone',
	backedUp: true,
	createdMs: Date.UTC(2026, 9, 2, 20),
	lastUsedMs: null,
	...over
});

/** The ceremony routes for stubApi: sign-in and registration end as `user`, with `key`. */
export function passkeyRoutes(user: User = leonard, key: PasskeyInfo = passkey()) {
	return {
		'POST /passkeys/login/start': { ceremony: 'c1', options: { publicKey: requestJson } },
		'POST /passkeys/login/finish': { success: true, user },
		'POST /passkeys/register/start': { ceremony: 'c2', options: { publicKey: creationJson } },
		'POST /passkeys/register/finish': { success: true, user, passkey: key }
	};
}
