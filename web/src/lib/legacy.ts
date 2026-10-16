// What older Maison builds left in a browser, freed: a service worker at /sw.js and caches named
// home-monitor-v* (the app has none today: live state only); a wall screen's service token, kept
// in storage before screens were paired (docs/passkeys.md « Wall screens »): no secret lingers.

import { stored, text } from '#lib/stored.ts';

/** Where a wall screen kept its token before pairing (no longer a key of `STORAGE`): read as
 * nothing, only forgotten. */
const oldScreenToken = stored<string>('maison-screen-token', '', text([]));

export function forgetLegacyScreenToken() {
	oldScreenToken.set(undefined);
}

export async function cleanupLegacyServiceWorker(): Promise<void> {
	if ('serviceWorker' in navigator) {
		const registrations = await navigator.serviceWorker.getRegistrations();
		await Promise.all(
			registrations
				.filter((r) => [r.active, r.waiting, r.installing].some((w) => w && new URL(w.scriptURL).pathname === '/sw.js'))
				.map((r) => r.unregister())
		);
	}
	if ('caches' in window) {
		const keys = await caches.keys();
		await Promise.all(keys.filter((k) => k.startsWith('home-monitor-v')).map((k) => caches.delete(k)));
	}
}
