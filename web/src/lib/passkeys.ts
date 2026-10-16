// Who comes in, said once: the session, the people, their passkeys (WebAuthn Level 3, as in
// Ariane: the server holds the ceremony, the browser only asks the authenticator; discoverable
// credentials, so signing in needs no name) and the invitations. docs/passkeys.md

import { ApiError, del, get, patch, path, post } from '#lib/api.ts';
import { m } from '#lib/paraglide/messages.js';
import { source } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';

/** Who is signed in. `role`: "admin" (may invite and configure) or "member". */
export interface User {
	id: string;
	name: string;
	role: string;
}

interface SessionResponse {
	success: boolean;
	user?: User;
	error?: string;
}

export const authApi = {
	verify: () => post<SessionResponse>('/auth/verify'),
	logout: () => post('/auth/logout'),
	/** Every session of mine ends, this one too. */
	logoutEverywhere: () => post('/auth/logout-everywhere')
};

/** One of my sign-ins, as the server keeps it (latest first). */
export interface SessionInfo {
	/** What names it to end it (not the session's secret). */
	id: string;
	createdMs: number;
	lastSeenMs: number;
	ip: string;
	userAgent: string;
	/** The device from its user agent (`iPhone`, `Mac`…); empty when unknown. */
	device: string;
	/** This browser's. */
	current: boolean;
}

export const sessionsApi = {
	list: () => get<{ sessions: SessionInfo[] }>('/auth/sessions').then((r) => r.sessions),
	/** One of my sign-ins ends; this browser's own too (its cookies cleared: signed out). */
	end: (id: string) => del(path`/auth/sessions/${id}`)
};

/** Someone who may come in (admin only): their passkeys counted. */
export interface Person {
	id: string;
	name: string;
	role: string;
	passkeys: number;
}

export const peopleApi = {
	list: () => get<{ people: Person[] }>('/people').then((r) => r.people),
	/** The person, their passkeys and sessions go (never oneself; a recent passkey first). */
	remove: (id: string) => withRecentPasskey(() => del<void>(path`/people/${id}`)),
	/** Every session of theirs ends (a lost phone); their passkeys stay. */
	endSessions: (id: string) => post(path`/people/${id}/sessions/end`)
};

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

/** Why passkeys cannot be used here, said with what to do, or null when they can: WebAuthn needs
 * a secure context (HTTPS, or http://localhost), and a browser that has it (a wall screen's WPE
 * has not). */
export function noPasskeys(): string | null {
	if (typeof window === 'undefined' || !window.isSecureContext) return m.pk_insecure();
	return 'PublicKeyCredential' in window ? null : m.pk_unsupported();
}

// base64url, for browsers without the Level 3 JSON helpers
const b64 = {
	decode(s: string): ArrayBuffer {
		const bin = atob(
			s
				.replace(/-/g, '+')
				.replace(/_/g, '/')
				.padEnd(Math.ceil(s.length / 4) * 4, '=')
		);
		return Uint8Array.from(bin, (c) => c.charCodeAt(0)).buffer;
	},
	encode(b: ArrayBuffer): string {
		return btoa(String.fromCharCode(...new Uint8Array(b)))
			.replace(/\+/g, '-')
			.replace(/\//g, '_')
			.replace(/=+$/, '');
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
	return {
		...x,
		challenge: b64.decode(x.challenge),
		allowCredentials: (x.allowCredentials ?? []).map(withBytes)
	} as unknown as PublicKeyCredentialRequestOptions;
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
	return {
		id: c.id,
		rawId: b64.encode(c.rawId),
		type: c.type,
		response,
		clientExtensionResults: c.getClientExtensionResults(),
		authenticatorAttachment: c.authenticatorAttachment
	};
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
			await PKC()
				.signalUnknownCredential?.({ rpId: publicKey.rpId, credentialId: credential.id })
				.catch(() => {});
		throw e;
	}
}

// Told who a confirmation signed in as (session.svelte.ts): true when it is still the person
// signed in. The page then holds the new sign-in's cookies.
let reconfirmed: (user: User | undefined) => boolean = () => true;
export function onReconfirmed(handler: (user: User | undefined) => boolean) {
	reconfirmed = handler;
}

/**
 * Sends `act`; when the server wants a recent passkey sign-in first (`reauth_needed`: adding,
 * renaming or removing a passkey, removing someone, an admin's invitation, pairing a screen),
 * says so, asks the passkey (a sign-in), then sends it again, once. Someone else's passkey
 * signs them in instead: nothing is sent for them, the refusal stands.
 */
export async function withRecentPasskey<T>(act: () => Promise<T>): Promise<T> {
	try {
		return await act();
	} catch (e) {
		if (!(e instanceof ApiError) || e.code !== 'reauth_needed') throw e;
		ui.say(m.reauth_ask());
		if (!reconfirmed((await signIn()).user)) throw e;
		return act();
	}
}

/**
 * Creates a passkey: from an invitation (a new person), or for the signed-in person (asking the
 * passkey again first when the last sign-in is old: `withRecentPasskey`).
 */
export async function register(opts: { invite?: string; name?: string } = {}): Promise<Registered> {
	const start = opts.invite
		? await post<Started>('/passkeys/register/start', { invite: opts.invite })
		: await withRecentPasskey(() => post<Started>('/passkeys/register/start', {}));
	const credential = (await navigator.credentials.create({
		publicKey: creationOptions(start.options.publicKey)
	})) as PublicKeyCredential | null;
	if (!credential) throw cancelled();
	return post<Registered>('/passkeys/register/finish', { ceremony: start.ceremony, credential: toJSON(credential), name: opts.name });
}

const listPasskeys = () => get<{ passkeys: PasskeyInfo[] }>('/passkeys').then((r) => r.passkeys);
export const renamePasskey = (key: string, name: string) => withRecentPasskey(() => patch<PasskeyInfo>(path`/passkeys/${key}`, { name }));
/** The passkey goes, and every session it may have opened: this browser gets fresh cookies. */
export const removePasskey = (key: string) => withRecentPasskey(() => del<void>(path`/passkeys/${key}`));

/** Who an invitation is for: its token in a body, never in an address. */
export const inviteGreeting = (token: string) => post<{ name: string }>('/invites/greet', { token });
const listInvites = () => get<{ invites: Invite[] }>('/invites').then((r) => r.invites);
/** A link for `name`; with `person`, a way back in for someone who lost every passkey (without
 * it, a name already someone's is refused: `person_exists`). An admin's or a recovery wants a
 * recent passkey sign-in. */
export const createInvite = (name: string, admin: boolean, person?: string) =>
	withRecentPasskey(() => post<CreatedInvite>('/invites', person ? { name, admin, person } : { name, admin }));
export const revokeInvite = (invite: string) => del<void>(path`/invites/${invite}`);

// what the account page reads, each under one key; they change only from that page, which
// reads them again
export const passkeysData = source('account:passkeys', listPasskeys);
export const peopleData = source('account:people', peopleApi.list);
export const invitesData = source('account:invites', listInvites);
export const sessionsData = source('account:sessions', sessionsApi.list);
