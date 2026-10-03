// Scenes' API: a named list of the remote's actions (backend/src/scenes.rs).

import { get, post, put, del, path } from '#lib/api.ts';
import type { IrAction } from '#lib/devices/remote/api.ts';
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

/** One entry per action run, as `/ir/test` says it (« ok: … » / « failed: … »). */
export interface SceneRunResponse {
	success: boolean;
	/** « 3/4 actions ok » */
	message?: string;
	results: string[];
}

export const scenesApi = {
	list: () => get<ScenesResponse>('/scenes'),
	save: (scene: Scene) =>
		put<{ success: boolean; scene: Scene }>(path`/scenes/${scene.id}`, { name: scene.name, icon: scene.icon, actions: scene.actions }),
	remove: (id: string) => del(path`/scenes/${id}`),
	run: (id: string) => post<SceneRunResponse>(path`/scenes/${id}/run`)
};
