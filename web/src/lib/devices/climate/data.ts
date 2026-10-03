// What the air conditioning views read, each under one key, once (docs/ux.md § 4): the IR
// blaster found on the network (the dashboard tile and the remote's pickers share it), the
// last order the house sent, and the blaster's learned codes.

import { source } from '#lib/live.svelte.ts';
import { broadlinkApi, type BroadlinkDiscoverResponse } from './api.ts';

/** Ask the network every 4 s until the RM4 Pro answers, for 2 min at most. */
export const SEARCH_EVERY_MS = 4_000;
export const SEARCH_FOR_MS = 120_000;
/** Someone may use the physical remote or the IR remote's climate key meanwhile. */
export const STATE_EVERY_MS = 60_000;

/** No blaster after this search's 2 min: the search stops (a new one asks again). */
export const searchOver = (d: BroadlinkDiscoverResponse | undefined, since: number) =>
	!d?.devices.length && Date.now() - since >= SEARCH_FOR_MS;

/** The blasters; `refresh(true)` is a new scan (an admin's), which starts a new 2 min search. */
export const blasters = source(
	'broadlink:discover',
	(force?: boolean) => broadlinkApi.discover(force),
	(d, since) => (d?.devices.length || searchOver(d, since) ? 0 : SEARCH_EVERY_MS)
);
export const climateState = source('broadlink:climate-state', broadlinkApi.getMitsubishiState, STATE_EVERY_MS);
/** The learned codes change only from the blaster's own tools: asked once. */
export const codes = source('broadlink:codes', broadlinkApi.listCodes);
