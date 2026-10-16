// What the plug views read, each under one key with its one fetch, and how often (docs/ux.md
// § 4: 5 s for plugs). Keys start with « meross » so one `refresh(MEROSS)` after a gesture asks
// them all again.

import { source, sources } from '#lib/live.svelte.ts';
import { MEROSS_CONSUMPTION_MS, MEROSS_ELECTRICITY_MS, MEROSS_LIST_MS, MEROSS_STATUS_MS } from '#lib/paces.ts';
import { merossApi } from './api.ts';

export const MEROSS = 'meross';

export const plugs = source(`${MEROSS}:list`, merossApi.list, MEROSS_LIST_MS);
export const status = sources((id: string) => source(`${MEROSS}:status:${id}`, () => merossApi.status(id), MEROSS_STATUS_MS));
/** A plug's live power: always asked (a view that should not ask an offline plug does not
 * mount it). */
export const electricity = sources((id: string) =>
	source(`${MEROSS}:electricity:${id}`, () => merossApi.electricity(id), MEROSS_ELECTRICITY_MS)
);
export const consumption = sources((id: string) =>
	source(`${MEROSS}:consumption:${id}`, () => merossApi.consumption(id), MEROSS_CONSUMPTION_MS)
);
