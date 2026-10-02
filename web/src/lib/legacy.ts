// Older Maison builds shipped a service worker at /sw.js and caches named home-monitor-v*.
// The app has none today (live state only); a browser that still runs the old one is freed.

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
