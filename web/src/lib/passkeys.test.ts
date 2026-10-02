import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { ApiError } from '#lib/api.ts';
import { stubApi } from '#lib/test/api.ts';
import { json } from '#lib/test/fetch.ts';
import { creationJson, credentialJson, leonard, passkeyRoutes, requestJson } from '#lib/test/passkeys.ts';
import {
	createInvite,
	creationOptions,
	inviteGreeting,
	listInvites,
	listPasskeys,
	passkeyMessage,
	register,
	removePasskey,
	renamePasskey,
	requestOptions,
	revokeInvite,
	signIn,
	toJSON,
	worded
} from './passkeys.ts';

const bytes = (b: BufferSource) => [...new Uint8Array(b as ArrayBuffer)];
const text = (s: string) => [...new TextEncoder().encode(s)];

/** A browser without the WebAuthn Level 3 JSON helpers, and an authenticator that answers. */
function browser(answer: unknown = { toJSON: () => credentialJson, id: credentialJson.id }) {
	const signal = vi.fn(async () => {});
	vi.stubGlobal('PublicKeyCredential', class { static signalUnknownCredential = signal; });
	vi.stubGlobal('AuthenticatorAttestationResponse', class {});
	const credentials = { get: vi.fn(async () => answer), create: vi.fn(async () => answer) };
	vi.stubGlobal('navigator', { credentials });
	return { credentials, signal };
}

describe('passkeys: the options, read without the browser’s JSON helpers', () => {
	beforeEach(() => void browser());

	it('turns the base64url fields of a creation into bytes, the rest as is', () => {
		const o = creationOptions({ ...creationJson, excludeCredentials: [{ type: 'public-key', id: 'Y3JlZA' }] });
		expect(bytes(o.challenge)).toEqual(text('challenge-challenge'));
		expect(bytes(o.user.id)).toEqual(text('user-handle'));
		expect(bytes(o.excludeCredentials![0].id)).toEqual(text('cred'));
		expect(o.authenticatorSelection?.residentKey).toBe('required');
	});

	it('turns the base64url fields of a request into bytes', () => {
		const o = requestOptions({ ...requestJson, allowCredentials: [{ type: 'public-key', id: 'Y3JlZA' }] });
		expect(bytes(o.challenge)).toEqual(text('challenge-challenge'));
		expect(bytes(o.allowCredentials![0].id)).toEqual(text('cred'));
		expect(o.rpId).toBe('localhost');
	});

	it('writes an assertion as JSON when the credential cannot', () => {
		const buf = (s: string) => new TextEncoder().encode(s).buffer;
		const assertion = {
			id: 'Y3JlZA',
			rawId: buf('cred'),
			type: 'public-key',
			authenticatorAttachment: 'platform',
			getClientExtensionResults: () => ({}),
			response: { clientDataJSON: buf('{}'), authenticatorData: buf('data'), signature: buf('sig'), userHandle: buf('user-handle') }
		} as unknown as PublicKeyCredential;
		expect(toJSON(assertion)).toEqual({
			id: 'Y3JlZA',
			rawId: 'Y3JlZA',
			type: 'public-key',
			response: { clientDataJSON: 'e30', authenticatorData: 'ZGF0YQ', signature: 'c2ln', userHandle: 'dXNlci1oYW5kbGU' },
			clientExtensionResults: {},
			authenticatorAttachment: 'platform'
		});
	});
});

describe('passkeys: the ceremonies', () => {
	afterEach(() => vi.unstubAllGlobals());

	it('signs in: a modal request, then the assertion sent back with its ceremony', async () => {
		const { credentials } = browser();
		const api = stubApi(passkeyRoutes());
		expect(await signIn()).toEqual({ success: true, user: leonard });
		expect(api.sent('POST', '/passkeys/login/start')[0].body).toEqual({ conditional: false });
		expect(credentials.get).toHaveBeenCalledWith(expect.objectContaining({ mediation: undefined }));
		expect(api.sent('POST', '/passkeys/login/finish')[0].body).toEqual({ ceremony: 'c1', credential: credentialJson });
	});

	it('a closed dialog is « cancelled », and nothing is sent', async () => {
		browser(null);
		const api = stubApi(passkeyRoutes());
		const refused = await signIn().catch((e) => e);
		expect(passkeyMessage(refused)).toBe(m.pk_cancelled());
		expect(api.sent('POST', '/passkeys/login/finish')).toEqual([]);
	});

	it('a passkey removed here: the password manager is told to forget it', async () => {
		const { signal } = browser();
		stubApi({ ...passkeyRoutes(), 'POST /passkeys/login/finish': json({ error: 'Unknown passkey', code: 'unknown_passkey' }, 404) });
		const refused = await signIn().catch((e) => e);
		expect(refused).toMatchObject({ code: 'unknown_passkey' });
		expect(signal).toHaveBeenCalledWith({ rpId: 'localhost', credentialId: credentialJson.id });
	});

	it('registers from an invitation, the token sent first', async () => {
		const { credentials } = browser();
		const api = stubApi(passkeyRoutes());
		const r = await register({ invite: 'tok' });
		expect(r.user).toEqual(leonard);
		expect(api.sent('POST', '/passkeys/register/start')[0].body).toEqual({ invite: 'tok' });
		expect(credentials.create).toHaveBeenCalledOnce();
		expect(api.sent('POST', '/passkeys/register/finish')[0].body).toEqual({ ceremony: 'c2', credential: credentialJson });
	});

	it('adds one more for the person signed in: no token', async () => {
		browser();
		const api = stubApi(passkeyRoutes());
		await register();
		expect(api.sent('POST', '/passkeys/register/start')[0].body).toEqual({});
	});
});

describe('passkeys: the rest of the API', () => {
	const cases: [string, () => Promise<unknown>, string, string, unknown?][] = [
		['list', () => listPasskeys(), 'GET', '/passkeys'],
		['rename', () => renamePasskey('k 1', 'Mac'), 'PATCH', '/passkeys/k%201', { name: 'Mac' }],
		['remove', () => removePasskey('k1'), 'DELETE', '/passkeys/k1'],
		['greeting', () => inviteGreeting('tok'), 'GET', '/invites/tok'],
		['invitations', () => listInvites(), 'GET', '/invites'],
		['invite', () => createInvite('Alex', false), 'POST', '/invites', { name: 'Alex', admin: false }],
		['revoke', () => revokeInvite('i1'), 'DELETE', '/invites/i1']
	];
	it.each(cases)('%s', async (_, call, method, path, body) => {
		const api = stubApi({ [`${method} ${path}`]: {} });
		await call();
		expect(api.calls).toEqual([{ method, path, body }]);
	});
});

describe('passkeys: what went wrong, in words', () => {
	it('words each refusal of the server and of the browser', () => {
		const coded = (code: string) => new ApiError('x', 400, code);
		const cases: [unknown, string][] = [
			[coded('passkey_rejected'), m.pk_rejected()],
			[coded('passkey_exists'), m.pk_exists()],
			[coded('invite_invalid'), m.pk_invite_invalid()],
			[coded('ceremony_expired'), m.pk_expired()],
			[coded('unknown_passkey'), m.pk_unknown()],
			[coded('last_passkey'), m.pk_last()],
			[coded('too_many_attempts'), m.pk_too_many()],
			[coded('passkey_off'), m.pk_off()],
			[new DOMException('closed', 'NotAllowedError'), m.pk_cancelled()],
			[new DOMException('aborted', 'AbortError'), m.pk_cancelled()],
			[new DOMException('wrong site', 'SecurityError'), m.pk_insecure()],
			[new DOMException('again', 'InvalidStateError'), m.pk_exists()],
			[new ApiError('TV unreachable', 503), 'TV unreachable'],
			['?', m.common_error()]
		];
		for (const [e, said] of cases) expect(passkeyMessage(e)).toBe(said);
	});

	it('rewords a failure for a gesture to show', async () => {
		await expect(worded(Promise.reject(new ApiError('x', 409, 'last_passkey')))).rejects.toThrow(m.pk_last());
		await expect(worded(Promise.resolve(1))).resolves.toBe(1);
	});
});
