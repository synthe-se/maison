// The photos, under one key: they change from this page, which reads them again (no polling).

import { source } from '#lib/live.svelte.ts';
import { photosApi } from './api.ts';

export const photos = source('photos:list', photosApi.list);
