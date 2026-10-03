// Scenes (« Je pars », « Nuit », « Film »): a named list of the remote's actions, run in
// order by the backend's engine. What the dashboard group, the editor and the app shortcuts
// (`/?scene=<id>`) share: the list, running one and saying how it went, the icons offered,
// the templates offered when there is none, and a new scene's id.

import { m } from '#lib/paraglide/messages.js';
import { refresh } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import type { Gesture } from '#lib/gesture.svelte.ts';
import { CONFIRM, FAILURE, haptic } from '#lib/haptics.ts';
import type { IconName } from '#lib/components/Icon.svelte';
import { outcome, toRows, type Row, type Sources } from '#lib/devices/remote/actions.ts';
import type { IrAction } from '#lib/devices/remote/api.ts';
import { HUE, ZIGBEE } from '#lib/devices/lamps/lamp.ts';
import { MEROSS } from '#lib/devices/meross/data.ts';
import { SHUTTERS } from '#lib/devices/shutters/data.ts';
import { BOX, TV } from '#lib/devices/tv/data.ts';
import { scenesApi, type Scene, type SceneRunResponse } from './api.ts';

/** The icons a scene may take, each with what it suggests (the picker's words). */
export const SCENE_ICONS: { icon: IconName; label: () => string }[] = [
	{ icon: 'log-out', label: m.scenes_icon_leave },
	{ icon: 'moon', label: m.scenes_icon_night },
	{ icon: 'film', label: m.scenes_icon_film },
	{ icon: 'sun', label: m.scenes_icon_morning },
	{ icon: 'house', label: m.scenes_icon_home },
	{ icon: 'sparkles', label: m.scenes_icon_mood }
];

/** A scene's icon as the app draws it (one it does not know: the sparkles). */
export const iconOf = (s: Pick<Scene, 'icon'>): IconName => SCENE_ICONS.find((i) => i.icon === s.icon)?.icon ?? 'sparkles';

/** A scene as its form holds it (its id aside). */
export interface SceneForm {
	name: string;
	icon: IconName;
	rows: Row[];
}

export const sceneForm = (s: Scene): SceneForm => ({ name: s.name, icon: iconOf(s), rows: toRows(s.actions) });

/** The ids of the templates, fixed: the app's shortcuts point at them (`/?scene=je-pars`). */
export const TEMPLATE_IDS = { leave: 'je-pars', night: 'nuit', film: 'film' } as const;

/** A dimmed lamp for a film: a fifth of the Zigbee maximum (254), a fifth for a Hue lamp. */
const DIM = { zigbee: 51, hue: 20 };

/**
 * The three scenes offered when there is none, filled from the house's devices: « Je pars »
 * (every lamp and plug off, the TV and the air conditioner off, every shutter closed),
 * « Nuit » (every lamp off, every shutter closed), « Film » (the TV on, the box's first app,
 * a lamp dimmed).
 */
export function templates(s: Sources): Scene[] {
	const lampsOff: IrAction[] = [
		...s.lamps.map((l): IrAction => ({ action: 'zigbee_power', lamp: l.id, state: 'off' })),
		...s.hueLamps.map((l): IrAction => ({ action: 'hue_power', lamp: l.id, state: 'off' }))
	];
	const plugsOff: IrAction[] = s.plugs.map((p) => ({ action: 'meross_power', device: p.id, state: 'off' }));
	const acOff: IrAction[] = s.hosts[0] ? [{ action: 'climate_off', host: s.hosts[0].host }] : [];
	const closed: IrAction[] = s.covers.map((c) => ({ action: 'cover', cover: c.id, command: 'close' }));
	const dim: IrAction[] = s.lamps[0]
		? [{ action: 'zigbee_brightness', lamp: s.lamps[0].id, brightness: DIM.zigbee }]
		: s.hueLamps[0]
			? [{ action: 'hue_brightness', lamp: s.hueLamps[0].id, brightness: DIM.hue }]
			: [];
	const film: IrAction[] = [
		{ action: 'tv_power', state: 'on', switchToBox: true },
		...(s.apps[0] ? [{ action: 'androidtv_app', package: s.apps[0].package } as IrAction] : []),
		...dim
	];
	return [
		{
			id: TEMPLATE_IDS.leave,
			name: m.scenes_template_leave(),
			icon: 'log-out',
			actions: [...lampsOff, ...plugsOff, { action: 'tv_power', state: 'off' }, ...acOff, ...closed]
		},
		{ id: TEMPLATE_IDS.night, name: m.scenes_template_night(), icon: 'moon', actions: [...lampsOff, ...closed] },
		{ id: TEMPLATE_IDS.film, name: m.scenes_template_film(), icon: 'film', actions: film }
	];
}

/** A new scene's id from its name (« Je pars » → « je-pars »), not one of `taken`. */
export function slug(name: string, taken: string[]): string {
	const base =
		name
			.normalize('NFD')
			.replace(/[̀-ͯ]/g, '')
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '')
			.slice(0, 28) || 'scene';
	let id = base;
	for (let n = 2; taken.includes(id); n++) id = `${base}-${n}`;
	return id;
}

/** How a run went, said once: every action done, or which ones failed and what they said. */
export function sayRun(scene: Pick<Scene, 'name'>, r: SceneRunResponse) {
	const failed = r.results.map(outcome).filter((o) => !o.ok);
	if (!failed.length) return ui.toast(m.scenes_done({ name: scene.name, count: r.results.length }));
	haptic(FAILURE);
	ui.toast(
		m.scenes_failed({ name: scene.name, count: failed.length, total: r.results.length, details: failed.map((f) => f.detail).join(' · ') }),
		{ warn: true }
	);
}

/** Runs `scene` through the caller's gesture (keyed by its id), says how it went, then reads
 * again what it may have changed. */
export function runScene(g: Gesture, scene: Scene) {
	haptic(CONFIRM);
	return g.run(
		() => scenesApi.run(scene.id),
		async (r) => {
			sayRun(scene, r);
			await Promise.all([HUE, ZIGBEE, MEROSS, SHUTTERS, TV, BOX].map((k) => refresh(k)));
		},
		scene.id
	);
}
