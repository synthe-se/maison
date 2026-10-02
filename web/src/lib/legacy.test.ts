import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanupLegacyServiceWorker } from './legacy.ts';

/** A service worker registration whose worker runs `script`. */
function registration(script: string, slot: 'active' | 'waiting' | 'installing' = 'active') {
	return {
		active: null as { scriptURL: string } | null,
		waiting: null as { scriptURL: string } | null,
		installing: null as { scriptURL: string } | null,
		[slot]: { scriptURL: `https://maison.example${script}` },
		unregister: vi.fn(async () => true)
	};
}

describe('cleanupLegacyServiceWorker', () => {
	afterEach(() => vi.unstubAllGlobals());

	it('unregisters the old /sw.js worker, whatever its state, and only it', async () => {
		const old = registration('/sw.js');
		const waiting = registration('/sw.js', 'waiting');
		const other = registration('/other-sw.js');
		vi.stubGlobal('navigator', { serviceWorker: { getRegistrations: async () => [old, waiting, other] } });
		vi.stubGlobal('window', {});
		await cleanupLegacyServiceWorker();
		expect(old.unregister).toHaveBeenCalledOnce();
		expect(waiting.unregister).toHaveBeenCalledOnce();
		expect(other.unregister).not.toHaveBeenCalled();
	});

	it('deletes the old home-monitor caches, and keeps the others', async () => {
		const del = vi.fn(async () => true);
		vi.stubGlobal('navigator', {});
		vi.stubGlobal('window', { caches: {} });
		vi.stubGlobal('caches', { keys: async () => ['home-monitor-v3', 'home-monitor-v12', 'fonts'], delete: del });
		await cleanupLegacyServiceWorker();
		expect(del.mock.calls).toEqual([['home-monitor-v3'], ['home-monitor-v12']]);
	});

	it('does nothing in a browser without service workers or caches', async () => {
		vi.stubGlobal('navigator', {});
		vi.stubGlobal('window', {});
		await expect(cleanupLegacyServiceWorker()).resolves.toBeUndefined();
	});
});
