// Generated from api/src/codes.rs (`cargo run --manifest-path api/Cargo.toml --bin shared-ts codes >
// web/src/lib/codes.ts`): never edited. Every refusal the API answers, by name.

export const CODES = [
	'not_signed_in',
	'forbidden',
	'not_for_screens',
	'bad_name',
	'bad_person',
	'passkey_off',
	'invite_invalid',
	'ceremony_expired',
	'passkey_rejected',
	'unknown_passkey',
	'passkey_exists',
	'last_passkey',
	'not_found',
	'reauth_needed',
	'person_exists',
	'not_yourself',
	'pairing_unknown',
	'pairing_elsewhere',
	'screen_exists',
	'too_many_attempts',
	'nested_scene',
	'too_many_actions',
	'not_a_device',
	'invalid',
	'not_configured',
	'busy',
	'unsupported',
	'device_unreachable',
	'service_unreachable',
	'internal',
	'use_public_address',
	'cross_site',
	'lan_only',
	'too_large'
] as const;

/** A refusal's name. */
export type Code = (typeof CODES)[number];
