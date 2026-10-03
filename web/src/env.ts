// Values the shell needs before any script runs (app.html's %sveltekit.env.*%), also importable
// from `$app/env/public`: each is said here once.
import { defineEnvVars } from '@sveltejs/kit/env';
import { readFileSync } from 'node:fs';

/** Where the browser keeps what the app remembers (localStorage), each key once: the app reads
 * them through `$app/env/public` (`#lib/stored.ts`), vite.config.ts hands the locale's to
 * Paraglide. */
export const STORAGE = { theme: 'maison-theme', locale: 'maison-locale', tvOrder: 'maison-tv-last-order' } as const;

export const variables = defineEnvVars({
	/** The page's lang before the locale is resolved: the base locale, never written in the code. */
	PUBLIC_BASE_LOCALE: {
		public: true,
		static: true,
		schema: () => JSON.parse(readFileSync(new URL('../../i18n/project.inlang/settings.json', import.meta.url), 'utf8')).baseLocale as string
	},
	/** The chosen theme, read before first paint and by `ui`. */
	PUBLIC_THEME_KEY: { public: true, static: true, schema: () => STORAGE.theme },
	/** The chosen language (Paraglide's localStorage strategy). */
	PUBLIC_LOCALE_KEY: { public: true, static: true, schema: () => STORAGE.locale },
	/** The TV's last power order, said while its state is assumed. */
	PUBLIC_TV_ORDER_KEY: { public: true, static: true, schema: () => STORAGE.tvOrder }
});
