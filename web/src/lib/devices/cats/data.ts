// The cats' corner (Tuya, local): what its views read, each under one key, as often as Tuya
// allows (docs/ux.md § 4: 10 to 15 s). Every key starts with TUYA: one `refresh(TUYA)` after a
// gesture (a connection switched) reads them all again.

import { source, sources, type Source } from '#lib/live.svelte.ts';
import { FEEDER_MS, FOUNTAIN_MS, LITTER_BOX_MS, TUYA_DEVICES_MS } from '#lib/paces.ts';
import {
	devicesApi,
	feederApi,
	fountainApi,
	litterBoxApi,
	type Device,
	type FeederStatus,
	type FountainStatus,
	type LitterBoxStatus
} from './api.ts';

export const TUYA = 'tuya:';

export type Kind = Device['type'];
export type StatusKind = Exclude<Kind, 'unknown'>;

/** What each kind's status endpoint says, parsed by the backend. */
interface Statuses {
	feeder: FeederStatus;
	fountain: FountainStatus;
	'litter-box': LitterBoxStatus;
}

/** Each kind's status pace (the shared paces: paces.ts). */
const PACE = { feeder: FEEDER_MS, fountain: FOUNTAIN_MS, 'litter-box': LITTER_BOX_MS } as const;

const STATUS = { feeder: feederApi.status, fountain: fountainApi.status, 'litter-box': litterBoxApi.status } as const;

/** Every Tuya device, shared by the dashboard group, the « Maintenant » strip and the pages. */
export const devices = source(`${TUYA}devices`, devicesApi.list, TUYA_DEVICES_MS);

const statusOf = sources((kind: StatusKind, id: string) =>
	source(`${TUYA}${id}:status`, async () => (await STATUS[kind](id)).parsedStatus, PACE[kind])
);

/** One device's parsed status, polled while a view of it is open. */
export const status = <K extends StatusKind>(kind: K, id: string) => statusOf(kind, id) as Source<Statuses[K] | undefined>;

/** A feeder's scheduled meals: changed only from its page, which reads them again. */
export const mealPlan = sources((id: string) => source(`${TUYA}${id}:meal-plan`, () => feederApi.getMealPlan(id)));
