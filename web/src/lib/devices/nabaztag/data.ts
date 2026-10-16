// The rabbit, read every 2 min (docs/ux.md § 4).

import { source } from '#lib/live.svelte.ts';
import { NABAZTAG_MS } from '#lib/paces.ts';
import { nabaztagApi } from './api.ts';

export const rabbit = source('nabaztag:status', nabaztagApi.status, NABAZTAG_MS);
