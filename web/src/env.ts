// Values the shell needs before any script runs (app.html's %sveltekit.env.*%), also importable
// from `$app/env/public`: each is said here once.
import { defineEnvVars } from '@sveltejs/kit/env';
import { readFileSync } from 'node:fs';

export const variables = defineEnvVars({
	/** The page's lang before the locale is resolved: the base locale, never written in the code. */
	PUBLIC_BASE_LOCALE: {
		public: true,
		static: true,
		schema: () => JSON.parse(readFileSync(new URL('../../i18n/project.inlang/settings.json', import.meta.url), 'utf8')).baseLocale as string
	},
	/** Where the chosen theme is kept (localStorage), read before first paint and by `ui`. */
	PUBLIC_THEME_KEY: { public: true, static: true, schema: () => 'maison-theme' }
});
