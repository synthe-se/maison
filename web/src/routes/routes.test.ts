import { describe, expect, it } from 'vitest';
import * as layout from './+layout.ts';
import { load } from './login/+page.ts';
import { load as oldInvite } from './invite/[token]/+page.ts';

describe('the app shell', () => {
	it('renders in the browser only: build/index.html is the SPA shell the backend serves', () => {
		expect(layout.ssr).toBe(false);
		expect(layout.prerender).toBe(false);
	});

	it('sends an old /login bookmark home (signing in happens wherever you are)', () => {
		expect(() => load()).toThrow(expect.objectContaining({ status: 307, location: '/' }));
	});

	it('moves an old /invite/<token> link’s token into the fragment, where the invitation goes on', () => {
		expect(() => oldInvite({ params: { token: 'ab12' } })).toThrow(expect.objectContaining({ status: 307, location: '/invite#ab12' }));
	});
});
