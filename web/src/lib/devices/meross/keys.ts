// What the plug views ask, and how often (the old React cadences, docs/ux/tableau-de-bord.md § 4:
// 5 s for plugs). Keys start with « meross » so one `refresh('meross')` after a gesture asks
// them all again.

export const LIST_EVERY_MS = 5_000;
export const STATUS_EVERY_MS = 3_000;
export const ELECTRICITY_EVERY_MS = 5_000;
/** Daily totals move slowly. */
export const CONSUMPTION_EVERY_MS = 30_000;

export const LIST_KEY = 'meross';
export const statusKey = (id: string) => `meross-status:${id}`;
export const electricityKey = (id: string) => `meross-electricity:${id}`;
export const consumptionKey = (id: string) => `meross-consumption:${id}`;
