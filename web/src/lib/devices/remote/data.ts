// What the remote configurator reads: the keymap (changed only from its own page, which reads
// it again) and the set-top box's last key events, read once a second while a key is being
// captured (as the React page did).

import { source } from '#lib/live.svelte.ts';
import { irApi } from './api.ts';

export const CAPTURE_EVERY_MS = 1_000;

export const keymap = source('ir:keymap', irApi.keymap);
export const recent = source('ir:recent', irApi.recent, CAPTURE_EVERY_MS);
