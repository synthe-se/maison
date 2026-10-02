import { paraglideVitePlugin } from '@inlang/paraglide-js';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// runes everywhere except in libraries
				runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true)
			},
			// a pure SPA: the Rust backend serves build/ and falls back to index.html
			adapter: adapter({ fallback: 'index.html' }),
			// a deploy is noticed within a minute, and taken at the next harmless moment
			// (+layout.svelte: a navigation, or the app seen again)
			version: { pollInterval: 60_000 },
			// no offline shell: the dashboard is live state, a cached one would lie
			serviceWorker: { register: false }
		}),
		paraglideVitePlugin({
			project: '../i18n/project.inlang',
			outdir: './src/lib/paraglide',
			strategy: ['localStorage', 'preferredLanguage', 'baseLocale'],
			localStorageKey: 'maison-locale',
			emitTsDeclarations: true
		})
	],
	test: {
		expect: { requireAssertions: true },
		// every test owns its fake timers and stubs: nothing leaks into the next
		restoreMocks: true,
		unstubGlobals: true,
		coverage: {
			provider: 'v8',
			include: ['src/**/*.{ts,svelte}'],
			exclude: ['src/lib/paraglide/**', 'src/**/*.test.ts', 'src/env.ts', 'src/app.d.ts', 'src/test-setup.ts'],
			reporter: ['text-summary', 'html']
		},
		projects: [
			{
				// components and rune modules: a real browser, as they run
				test: {
					name: 'browser',
					include: ['src/**/*.svelte.test.ts'],
					setupFiles: ['src/test-setup.ts'],
					browser: { enabled: true, provider: playwright(), headless: true, instances: [{ browser: 'chromium' }] }
				}
			},
			{
				// plain modules: Node
				test: { name: 'node', environment: 'node', include: ['src/**/*.test.ts'], exclude: ['src/**/*.svelte.test.ts'] }
			}
		]
	},
	server: {
		// `make backend` on :3033
		proxy: {
			'/api': { target: 'http://127.0.0.1:3033', changeOrigin: true }
		}
	}
});
