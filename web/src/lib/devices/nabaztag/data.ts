// The rabbit, read every 2 min (docs/ux.md § 4).

import { source } from '#lib/live.svelte.ts';
import { nabaztagApi } from './api.ts';

export const POLL_MS = 120_000;
export const rabbit = source('nabaztag:status', nabaztagApi.status, POLL_MS);
