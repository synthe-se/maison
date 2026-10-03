import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { ApiError, UNREACHABLE } from '#lib/api.ts';
import { errorText } from './errors.ts';

describe('errorText', () => {
	it('words each refusal of the server and of the browser by its name', () => {
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
			[coded('forbidden'), m.admin_only()],
			[coded('not_signed_in'), m.error_signed_out()],
			[coded('bad_name'), m.error_bad_name()],
			[coded(UNREACHABLE), m.error_network()],
			[new DOMException('closed', 'NotAllowedError'), m.pk_cancelled()],
			[new DOMException('aborted', 'AbortError'), m.pk_cancelled()],
			[new DOMException('wrong site', 'SecurityError'), m.pk_insecure()],
			[new DOMException('again', 'InvalidStateError'), m.pk_exists()]
		];
		for (const [e, said] of cases) expect(errorText(e)).toBe(said);
	});

	it('keeps the server’s own words, with no « Erreur. » in front', () => {
		expect(errorText(new ApiError('TV unreachable', 503))).toBe('TV unreachable');
	});

	it('words a silent answer by its status', () => {
		expect(errorText(new ApiError('', 403))).toBe(m.admin_only());
		expect(errorText(new ApiError('', 404))).toBe(m.error_not_found());
		expect(errorText(new ApiError('', 502))).toBe(m.error_network());
		expect(errorText(new ApiError('', 500))).toBe(m.error_server());
	});

	it('says « no answer » when fetch found no server', () => {
		expect(errorText(new TypeError('Failed to fetch'))).toBe(m.error_network());
	});

	it('anything else: its message, or a plain sentence', () => {
		expect(errorText(new Error('Volet injoignable'))).toBe('Volet injoignable');
		expect(errorText('?')).toBe(m.error_server());
	});
});
