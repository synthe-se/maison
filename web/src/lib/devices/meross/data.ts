// What the plug views read, each under one key with its one fetch, and how often (docs/ux.md
// § 4: 5 s for plugs). Keys start with « meross » so one `refresh(MEROSS)` after a gesture asks
// them all again.

import { source, sources } from '#lib/live.svelte.ts';
import { merossApi } from './api.ts';

export const MEROSS = 'meross';
export const LIST_EVERY_MS = 5_000;
export const STATUS_EVERY_MS = 3_000;
export const ELECTRICITY_EVERY_MS = 5_000;
/** Daily totals move slowly. */
export const CONSUMPTION_EVERY_MS = 30_000;

export const plugs = source(`${MEROSS}:list`, merossApi.list, LIST_EVERY_MS);
export const status = sources((id: string) => source(`${MEROSS}:status:${id}`, () => merossApi.status(id), STATUS_EVERY_MS));
/** A plug's live power: always asked (a view that should not ask an offline plug does not
 * mount it). */
export const electricity = sources((id: string) =>
	source(`${MEROSS}:electricity:${id}`, () => merossApi.electricity(id), ELECTRICITY_EVERY_MS)
);
export const consumption = sources((id: string) =>
	source(`${MEROSS}:consumption:${id}`, () => merossApi.consumption(id), CONSUMPTION_EVERY_MS)
);
