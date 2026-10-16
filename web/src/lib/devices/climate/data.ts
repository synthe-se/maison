// What the air conditioning views read, each under one key, once (docs/ux.md § 4): the IR
// blaster found on the network (the dashboard tile and the remote's pickers share it), the
// last order the house sent, and the blaster's learned codes.

import { source } from '#lib/live.svelte.ts';
import { BROADLINK_SEARCH_FOR_MS, BROADLINK_SEARCH_MS, CLIMATE_STATE_MS } from '#lib/paces.ts';
import { broadlinkApi, type BroadlinkDiscoverResponse } from './api.ts';

/** Asked every BROADLINK_SEARCH_MS until the RM4 Pro answers. No blaster after this search's 2 min: the search stops (a new one asks again). */
export const searchOver = (d: BroadlinkDiscoverResponse | undefined, since: number) =>
	!d?.devices.length && Date.now() - since >= BROADLINK_SEARCH_FOR_MS;

/** The blasters; `refresh(true)` is a new scan (an admin's), which starts a new 2 min search. */
export const blasters = source(
	'broadlink:discover',
	(force?: boolean) => (force ? broadlinkApi.rediscover() : broadlinkApi.discover()),
	(d, since) => (d?.devices.length || searchOver(d, since) ? 0 : BROADLINK_SEARCH_MS)
);
export const climateState = source('broadlink:climate-state', broadlinkApi.getMitsubishiState, CLIMATE_STATE_MS);
/** The learned codes change only from the blaster's own tools: asked once. */
export const codes = source('broadlink:codes', broadlinkApi.listCodes);
