// The cats' corner (Tuya, local): what its views read, each under one key, as often as Tuya
// allows (docs/ux.md § 4: 10 to 15 s). Every key starts with TUYA: one `refresh(TUYA)` after a
// gesture (a connection switched) reads them all again.

import { source, sources, type Source } from '#lib/live.svelte.ts';
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
export interface Statuses {
	feeder: FeederStatus;
	fountain: FountainStatus;
	'litter-box': LitterBoxStatus;
}

/** Poll intervals, as the React app had them. */
export const POLL = { devices: 10_000, feeder: 15_000, fountain: 10_000, 'litter-box': 15_000 } as const;

const STATUS = { feeder: feederApi.status, fountain: fountainApi.status, 'litter-box': litterBoxApi.status } as const;

/** Every Tuya device, shared by the dashboard group, the « Maintenant » strip and the pages. */
export const devices = source(`${TUYA}devices`, devicesApi.list, POLL.devices);

const statusOf = sources((kind: StatusKind, id: string) =>
	source(`${TUYA}${id}:status`, async () => (await STATUS[kind](id)).parsedStatus, POLL[kind])
);

/** One device's parsed status, polled while a view of it is open. */
export const status = <K extends StatusKind>(kind: K, id: string) => statusOf(kind, id) as Source<Statuses[K] | undefined>;

/** A feeder's scheduled meals: changed only from its page, which reads them again. */
export const mealPlan = sources((id: string) => source(`${TUYA}${id}:meal-plan`, () => feederApi.getMealPlan(id)));
