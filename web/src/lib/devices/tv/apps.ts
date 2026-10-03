// The Android TV box's app shortcuts, once: its tile, its page and the remote's action
// pickers offer the same ones.

import type { AndroidApp, AndroidTvConfig } from '#lib/devices/tv/api.ts';

/** Apps worth a shortcut, keyed by the packages actually on the box (config can override). */
export const SHORTCUTS: AndroidApp[] = [
	{ package: 'org.smarttube.beta', label: 'SmartTube' },
	{ package: 'studio.kahn.iris.tv', label: 'Iris' }
];

/** The box's shortcuts: its configured favorites, else ours. */
export const boxApps = (config: AndroidTvConfig | undefined): AndroidApp[] =>
	config?.favoriteApps?.length ? config.favoriteApps : SHORTCUTS;
