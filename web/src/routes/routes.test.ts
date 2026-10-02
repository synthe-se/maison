import { describe, expect, it } from 'vitest';
import * as layout from './+layout.ts';
import { load } from './login/+page.ts';

describe('the app shell', () => {
	it('renders in the browser only: build/index.html is the SPA shell the backend serves', () => {
		expect(layout.ssr).toBe(false);
		expect(layout.prerender).toBe(false);
	});

	it('sends an old /login bookmark home (signing in happens wherever you are)', () => {
		expect(() => load()).toThrow(expect.objectContaining({ status: 307, location: '/' }));
	});
});
