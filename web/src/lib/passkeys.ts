// Passkeys (WebAuthn Level 3), as in Ariane. The server holds the ceremony; the browser only
// asks the authenticator. Discoverable credentials: signing in needs no name.
// docs/dependances/passkeys.md

import { ApiError, del, get, patch, path, post, type SessionResponse, type User } from '#lib/api.ts';

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

/** A ceremony begun by the server: its id and the browser's options. */
type Started = { ceremony: string; options: { publicKey: Json } };

/** Signs in with a passkey (also to confirm it is still me before adding one). */
export async function signIn(): Promise<SessionResponse> {
	const start = await post<Started>('/passkeys/login/start', {});
	const publicKey = requestOptions(start.options.publicKey);
	const credential = (await navigator.credentials.get({ publicKey })) as PublicKeyCredential | null;
	if (!credential) throw cancelled();
	try {
		return await post<SessionResponse>('/passkeys/login/finish', { ceremony: start.ceremony, credential: toJSON(credential) });
	} catch (e) {
		// the passkey was removed here: tell the password manager to forget it
		if (e instanceof ApiError && e.code === 'unknown_passkey' && publicKey.rpId)
			await PKC().signalUnknownCredential?.({ rpId: publicKey.rpId, credentialId: credential.id }).catch(() => {});
		throw e;
	}
}

/**
 * Creates a passkey: from an invitation (a new person), or for the signed-in person. Adding one
 * to a session whose last passkey sign-in is old asks the passkey again first (the server says
 * `reauth_needed`): then the registration starts again, once.
 */
export async function register(opts: { invite?: string; name?: string; onReauth?: () => void } = {}): Promise<Registered> {
	const begin = () => post<Started>('/passkeys/register/start', opts.invite ? { invite: opts.invite } : {});
	let start: Started;
	try {
		start = await begin();
	} catch (e) {
		if (opts.invite || !(e instanceof ApiError) || e.code !== 'reauth_needed') throw e;
		opts.onReauth?.();
		await signIn();
		start = await begin();
	}
	const credential = (await navigator.credentials.create({ publicKey: creationOptions(start.options.publicKey) })) as PublicKeyCredential | null;
	if (!credential) throw cancelled();
	return post<Registered>('/passkeys/register/finish', { ceremony: start.ceremony, credential: toJSON(credential), name: opts.name });
}

export const listPasskeys = () => get<PasskeyInfo[]>('/passkeys');
export const renamePasskey = (key: string, name: string) => patch<PasskeyInfo>(path`/passkeys/${key}`, { name });
export const removePasskey = (key: string) => del<void>(path`/passkeys/${key}`);

export const inviteGreeting = (token: string) => get<{ name: string }>(path`/invites/${token}`);
export const listInvites = () => get<Invite[]>('/invites');
/** A link for `name`; with `person`, a way back in for someone who lost every passkey (without
 * it, a name already someone's is refused: `person_exists`). */
export const createInvite = (name: string, admin: boolean, person?: string) =>
	post<CreatedInvite>('/invites', person ? { name, admin, person } : { name, admin });
export const revokeInvite = (invite: string) => del<void>(path`/invites/${invite}`);
