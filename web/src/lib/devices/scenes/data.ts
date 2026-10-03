// The scenes, under one key: they change only from the editor, which reads them again (no
// polling).

import { source } from '#lib/live.svelte.ts';
import { scenesApi } from './api.ts';

export const scenes = source('scenes:list', scenesApi.list);
