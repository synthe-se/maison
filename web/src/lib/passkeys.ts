// Passkeys (WebAuthn Level 3), as in Ariane. The server holds the ceremony; the browser only
// asks the authenticator. Discoverable credentials: signing in needs no name.
// docs/dependances/passkeys.md

import { api, ApiError, type SessionResponse, type User } from '#lib/api.ts';
import { m } from '#lib/paraglide/messages.js';

export interface PasskeyInfo {
	id: string;
	name: string;
	/** Synced by a keychain (iCloud, Google, a password manager). */
	backedUp: boolean;
	createdMs: number;
	lastUsedMs: number | null;
}

export interface Invite {
	id: string;
	person: string;
	name: string;
	role: string;
	expiresMs: number;
}

export interface CreatedInvite {
	id: string;
	/** The one-time link; shown once, only its hash is kept. */
	url: string;
	expiresMs: number;
}

export interface Registered {
	passkey: PasskeyInfo;
	user: User;
}

/** The person closed the system dialog. */
const cancelled = () => new ApiError('cancelled', 0, 'cancelled');

/** WebAuthn needs a secure context: HTTPS, or http://localhost. */
export const supported = () => typeof window !== 'undefined' && window.isSecureContext && 'PublicKeyCredential' in window;

// base64url, for browsers without the Level 3 JSON helpers
const b64 = {
	decode(s: string): ArrayBuffer {
		const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(s.length / 4) * 4, '='));
		return Uint8Array.from(bin, (c) => c.charCodeAt(0)).buffer;
	},
	encode(b: ArrayBuffer): string {
		return btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
	}
};

type Json = Record<string, unknown>;
const PKC = () =>
	PublicKeyCredential as unknown as {
		parseCreationOptionsFromJSON?: (o: Json) => PublicKeyCredentialCreationOptions;
		parseRequestOptionsFromJSON?: (o: Json) => PublicKeyCredentialRequestOptions;
		signalUnknownCredential?: (o: { rpId: string; credentialId: string }) => Promise<void>;
	};

/** The server's options as JSON: the binary fields in base64url. */
type WithId = Json & { id: string };
type CreationJson = Json & { challenge: string; user: WithId; excludeCredentials?: WithId[] };
type RequestJson = Json & { challenge: string; allowCredentials?: WithId[] };
const withBytes = (c: WithId) => ({ ...c, id: b64.decode(c.id) });

export function creationOptions(o: Json): PublicKeyCredentialCreationOptions {
	const parse = PKC().parseCreationOptionsFromJSON;
	if (parse) return parse(o);
	const x = o as CreationJson;
	// the JSON's shape is the browser's, with bytes where it has text
	return {
		...x,
		challenge: b64.decode(x.challenge),
		user: withBytes(x.user),
		excludeCredentials: (x.excludeCredentials ?? []).map(withBytes)
	} as unknown as PublicKeyCredentialCreationOptions;
}

export function requestOptions(o: Json): PublicKeyCredentialRequestOptions {
	const parse = PKC().parseRequestOptionsFromJSON;
	if (parse) return parse(o);
	const x = o as RequestJson;
	return { ...x, challenge: b64.decode(x.challenge), allowCredentials: (x.allowCredentials ?? []).map(withBytes) } as unknown as PublicKeyCredentialRequestOptions;
}

export function toJSON(c: PublicKeyCredential): Json {
	const cred = c as unknown as { toJSON?: () => Json };
	if (typeof cred.toJSON === 'function') return cred.toJSON();
	const response: Json = { clientDataJSON: b64.encode(c.response.clientDataJSON) };
	if (c.response instanceof AuthenticatorAttestationResponse) {
		response.attestationObject = b64.encode(c.response.attestationObject);
		response.transports = c.response.getTransports?.() ?? [];
	} else {
		const r = c.response as AuthenticatorAssertionResponse;
		response.authenticatorData = b64.encode(r.authenticatorData);
		response.signature = b64.encode(r.signature);
		response.userHandle = r.userHandle ? b64.encode(r.userHandle) : null;
	}
	return { id: c.id, rawId: b64.encode(c.rawId), type: c.type, response, clientExtensionResults: c.getClientExtensionResults(), authenticatorAttachment: c.authenticatorAttachment };
}

/** Signs in with a passkey. `conditional`: offered in the field's autofill, until aborted. */
export async function signIn(opts: { conditional?: boolean; signal?: AbortSignal } = {}): Promise<SessionResponse> {
	const start = await api<{ ceremony: string; options: { publicKey: Json; mediation?: CredentialMediationRequirement } }>('/passkeys/login/start', {
		method: 'POST',
		body: { conditional: !!opts.conditional }
	});
	const publicKey = requestOptions(start.options.publicKey);
	const credential = (await navigator.credentials.get({
		publicKey,
		mediation: opts.conditional ? 'conditional' : undefined,
		signal: opts.signal
	})) as PublicKeyCredential | null;
	if (!credential) throw cancelled();
	try {
		return await api<SessionResponse>('/passkeys/login/finish', { method: 'POST', body: { ceremony: start.ceremony, credential: toJSON(credential) } });
	} catch (e) {
		// the passkey was removed here: tell the password manager to forget it
		if (e instanceof ApiError && e.code === 'unknown_passkey' && publicKey.rpId)
			await PKC().signalUnknownCredential?.({ rpId: publicKey.rpId, credentialId: credential.id }).catch(() => {});
		throw e;
	}
}

/** Creates a passkey: from an invitation (a new person), or for the signed-in person. */
export async function register(opts: { invite?: string; name?: string } = {}): Promise<Registered> {
	const start = await api<{ ceremony: string; options: { publicKey: Json } }>('/passkeys/register/start', {
		method: 'POST',
		body: opts.invite ? { invite: opts.invite } : {}
	});
	const credential = (await navigator.credentials.create({ publicKey: creationOptions(start.options.publicKey) })) as PublicKeyCredential | null;
	if (!credential) throw cancelled();
	return api<Registered>('/passkeys/register/finish', { method: 'POST', body: { ceremony: start.ceremony, credential: toJSON(credential), name: opts.name } });
}

const id = encodeURIComponent;

export const listPasskeys = () => api<PasskeyInfo[]>('/passkeys');
export const renamePasskey = (key: string, name: string) => api<PasskeyInfo>(`/passkeys/${id(key)}`, { method: 'PATCH', body: { name } });
export const removePasskey = (key: string) => api<void>(`/passkeys/${id(key)}`, { method: 'DELETE' });

export const inviteGreeting = (token: string) => api<{ name: string }>(`/invites/${id(token)}`);
export const listInvites = () => api<Invite[]>('/invites');
export const createInvite = (name: string, admin: boolean) => api<CreatedInvite>('/invites', { method: 'POST', body: { name, admin } });
export const revokeInvite = (invite: string) => api<void>(`/invites/${id(invite)}`, { method: 'DELETE' });

/** What went wrong, said plainly, with what to do. */
export function passkeyMessage(e: unknown): string {
	const code = e instanceof ApiError ? e.code : e instanceof DOMException ? e.name : undefined;
	switch (code) {
		case 'NotAllowedError':
		case 'AbortError':
		case 'cancelled':
			return m.pk_cancelled();
		case 'SecurityError':
			return m.pk_insecure();
		case 'InvalidStateError':
		case 'passkey_exists':
			return m.pk_exists();
		case 'invite_invalid':
			return m.pk_invite_invalid();
		case 'ceremony_expired':
			return m.pk_expired();
		case 'passkey_rejected':
			return m.pk_rejected();
		case 'unknown_passkey':
			return m.pk_unknown();
		case 'last_passkey':
			return m.pk_last();
		case 'too_many_attempts':
			return m.pk_too_many();
		case 'passkey_off':
			return m.pk_off();
		default:
			return e instanceof Error && e.message ? e.message : m.common_error();
	}
}

/** `promise`, its failure worded by `passkeyMessage`: for a Gesture, which shows what it throws. */
export const worded = <T>(promise: Promise<T>): Promise<T> =>
	promise.catch((e) => {
		throw new Error(passkeyMessage(e));
	});
