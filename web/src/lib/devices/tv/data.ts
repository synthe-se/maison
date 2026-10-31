// The TV and the box, each read under one key and as slowly as the TV allows (TV_MS, once a
// minute, docs/ux.md § 4): their tiles, their pages and the remote's pickers share them. The
// TV's JointSPACE HTTP server is single-threaded and dies for good under bursts (only a mains
// power cycle revives it), so the dashboard asks rarely and the backend spaces every call it
// makes (900 ms between two, a status read is four of them). Never poll it faster.

import { source } from '#lib/live.svelte.ts';
import { androidTvApi, tvApi } from './api.ts';
import { TV_MS } from '#lib/paces.ts';

export const TV = 'tv';
export const BOX = 'androidtv';

export const tv = source(`${TV}:status`, tvApi.status, TV_MS);
// polled as slowly as the TV: waking the box drives the TV over CEC
export const box = source(`${BOX}:status`, androidTvApi.status, TV_MS);
// an admin's, in the box's settings
export const boxCleanup = source(`${BOX}:cleanup`, androidTvApi.cleanup, TV_MS);
