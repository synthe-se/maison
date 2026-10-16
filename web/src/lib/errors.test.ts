import { afterEach, describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { ApiError, UNREACHABLE } from '#lib/api.ts';
import { CODES } from '#lib/codes.ts';
import { CODE_WORDS, codeText, errorText } from './errors.ts';

afterEach(() => vi.restoreAllMocks());

describe('errorText', () => {
	it('words every refusal the API answers, each in words of its own', () => {
		const said = CODES.map((code) => errorText(new ApiError('Server words', 400, code)));
		for (const s of said) expect(s).not.toBe('Server words');
		expect(new Set(said).size).toBe(CODES.length);
		expect(Object.keys(CODE_WORDS).toSorted()).toEqual([...CODES].toSorted());
	});

	it('words each refusal by its name', () => {
		const coded = (code: string) => new ApiError('x', 400, code);
		const cases: [unknown, string][] = [
			[coded('passkey_rejected'), m.code_passkey_rejected()],
			[coded('passkey_exists'), m.code_passkey_exists()],
			[coded('forbidden'), m.code_forbidden()],
			[coded('not_signed_in'), m.code_not_signed_in()],
			[coded('device_unreachable'), m.code_device_unreachable()],
			[coded(UNREACHABLE), m.error_network()],
			[new DOMException('closed', 'NotAllowedError'), m.pk_cancelled()],
			[new DOMException('aborted', 'AbortError'), m.pk_cancelled()],
			[new DOMException('wrong site', 'SecurityError'), m.pk_insecure()],
			[new DOMException('again', 'InvalidStateError'), m.code_passkey_exists()]
		];
		for (const [e, said] of cases) expect(errorText(e)).toBe(said);
	});

	it('never shows the server’s own text: an unknown refusal is logged and worded by its status', () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		expect(errorText(new ApiError('TV unreachable', 503))).toBe(m.error_network());
		expect(errorText(new ApiError('Something new', 400, 'brand_new'))).toBe(m.error_server());
		expect(warn).toHaveBeenCalledTimes(2);
	});

	it('words a silent answer by its status', () => {
		vi.spyOn(console, 'warn').mockImplementation(() => {});
		expect(errorText(new ApiError('', 403))).toBe(m.code_forbidden());
		expect(errorText(new ApiError('', 404))).toBe(m.code_not_found());
		expect(errorText(new ApiError('', 502))).toBe(m.error_network());
		expect(errorText(new ApiError('', 500))).toBe(m.error_server());
	});

	it('says « no answer » when fetch found no server', () => {
		expect(errorText(new TypeError('Failed to fetch'))).toBe(m.error_network());
	});

	it('anything else: a plain sentence, never its message', () => {
		vi.spyOn(console, 'warn').mockImplementation(() => {});
		expect(errorText(new Error('Volet injoignable'))).toBe(m.error_server());
		expect(errorText('?')).toBe(m.error_server());
	});
});

describe('codeText', () => {
	it('the words of a known code, nothing for another', () => {
		expect(codeText('busy')).toBe(m.code_busy());
		expect(codeText('toString')).toBeUndefined();
		expect(codeText(undefined)).toBeUndefined();
	});
});
