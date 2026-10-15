// What the shutter views read, each under one key: every shutter, read slowly at rest and
// every second while a motor runs (so the position follows the travel), and the house's place
// for the sun schedule. One value for the group, the « Maintenant » strip and the remote's
// pickers. The place also says when the sun next rises and sets: read again just after, the
// wall screens' theme (SunTheme) follows it.

import { source } from '#lib/live.svelte.ts';
import { untilSun } from '#lib/sun.ts';
import { shuttersApi, type Shutter } from './api.ts';

export const SHUTTERS = 'shutters';
export const MOVING_EVERY_MS = 1_000;
export const AT_REST_EVERY_MS = 10_000;

export const moving = (c: Shutter) => c.motion === 'opening' || c.motion === 'closing';

export const covers = source(`${SHUTTERS}:list`, shuttersApi.list, (d) => (d?.covers.some(moving) ? MOVING_EVERY_MS : AT_REST_EVERY_MS));
/** Changed only from its own picker, which shows the answer; read again after each sunrise and
 * sunset. */
export const place = source(`${SHUTTERS}:place`, shuttersApi.place, (d) => untilSun(d?.sun));
