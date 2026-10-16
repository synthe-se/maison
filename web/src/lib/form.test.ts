import { describe, expect, it } from 'vitest';
import { missingFields } from './form.ts';

const words = { code: () => 'Le code manque', name: () => 'Le nom manque' };

describe('missingFields', () => {
	it('says each empty field, and the first of them', () => {
		expect(missingFields({ code: ' ', name: '' }, words)).toEqual({
			errors: { code: 'Le code manque', name: 'Le nom manque' },
			first: 'code'
		});
		expect(missingFields({ code: '1234', name: '' }, words)).toEqual({ errors: { name: 'Le nom manque' }, first: 'name' });
	});

	it('nothing missing: nothing said', () => {
		expect(missingFields({ code: '1234', name: 'Salon' }, words)).toEqual({ errors: {} });
	});
});
