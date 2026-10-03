import { afterEach } from 'vitest';
import { forgetAll } from '#lib/live.svelte.ts';
// the tokens the components' styles read (their sizes: a switch, a slider's thumb)
import './styles/tokens.css';

// The browser tests run outside SvelteKit's dev server: its generated `$app/env/public`
// module reads a global the server would have set (when a test file loads it, after this).
// The values themselves are inlined there.
(globalThis as { __sveltekit_dev?: unknown }).__sveltekit_dev ??= { env: {} };

// Each test file starts from a clean browser: a locale or a theme chosen by an earlier file
// must not leak into the next.
try {
	localStorage.clear();
} catch {
	// no storage: nothing to leak
}

// Each test starts from no live value: what one test read never answers the next.
afterEach(() => forgetAll());
