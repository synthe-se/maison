// What the shutter views read, each under one key: every shutter, read slowly at rest and
// every second while a motor runs (so the position follows the travel), and the house's place
// for the sun schedule. One value for the group, the « Maintenant » strip and the remote's
// pickers. The place also says when the sun next rises and sets: read again just after, the
// wall screens' theme (SunTheme) follows it.

import { source } from '#lib/live.svelte.ts';
import { SHUTTERS_AT_REST_MS, SHUTTERS_MOVING_MS } from '#lib/paces.ts';
import { untilSun } from '#lib/sun/api.ts';
import { shuttersApi, type Shutter } from './api.ts';

export const SHUTTERS = 'shutters';

export const moving = (c: Shutter) => c.motion === 'opening' || c.motion === 'closing';

export const covers = source(`${SHUTTERS}:list`, shuttersApi.list, (d) =>
	d?.covers.some(moving) ? SHUTTERS_MOVING_MS : SHUTTERS_AT_REST_MS
);
/** Changed only from its own picker, which shows the answer; read again after each sunrise and
 * sunset. */
export const place = source(`${SHUTTERS}:place`, shuttersApi.place, (d) => untilSun(d?.sun));
