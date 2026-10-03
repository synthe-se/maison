// The TV and the box, each read under one key and as slowly as the TV allows (REMOTE_POLL):
// their tiles, their pages and the remote's pickers share them.

import { source } from '#lib/live.svelte.ts';
import { androidTvApi, tvApi } from './api.ts';
import { REMOTE_POLL } from './remote.ts';

export const TV = 'tv';
export const BOX = 'androidtv';

export const tv = source(`${TV}:status`, tvApi.status, REMOTE_POLL);
// polled as slowly as the TV: waking the box drives the TV over CEC
export const box = source(`${BOX}:status`, androidTvApi.status, REMOTE_POLL);
