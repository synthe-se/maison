// Scenes' API: a named list of the remote's actions (backend/src/scenes.rs).

import { get, post, put, del, path } from '#lib/api.ts';
import type { IrAction, KeyResponse } from '#lib/devices/remote/api.ts';
export interface Scene {
	/** A slug, `[a-z0-9-]{1,32}`. */
	id: string;
	name: string;
	/** One of the app's Icon names. */
	icon: string;
	actions: IrAction[];
}

export interface ScenesResponse {
	success: boolean;
	scenes: Scene[];
}

export const scenesApi = {
	list: () => get<ScenesResponse>('/scenes'),
	save: (scene: Scene) =>
		put<{ success: boolean; scene: Scene }>(path`/scenes/${scene.id}`, { name: scene.name, icon: scene.icon, actions: scene.actions }),
	remove: (id: string) => del(path`/scenes/${id}`),
	run: (id: string) => post<KeyResponse>(path`/scenes/${id}/run`)
};
