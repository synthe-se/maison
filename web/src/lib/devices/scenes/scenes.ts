// Scenes (« Je pars », « Nuit », « Film »): a named list of actions, run in order by the
// backend. What the dashboard group, the editor and the app shortcuts
// (`/?scene=<id>`) share: the list, running one and saying how it went, the icons offered,
// the templates offered when there is none, and a new scene's id.

import { m } from '#lib/paraglide/messages.js';
import { refresh } from '#lib/live.svelte.ts';
import type { Gesture } from '#lib/gesture.svelte.ts';
import { facts } from '#lib/i18n.svelte.ts';
import type { IconName } from '#lib/components/Icon.svelte';
import { outcome, toRows, type Row, type Sources } from '#lib/devices/actions/actions.ts';
import type { Action, RunResponse } from '#lib/devices/actions/api.ts';
import { sayOutcome } from '#lib/devices/everyone.ts';
import { HUE, ZIGBEE } from '#lib/devices/lamps/lamp.ts';
import { MEROSS } from '#lib/devices/meross/data.ts';
import { SHUTTERS } from '#lib/devices/shutters/data.ts';
import { BOX, TV } from '#lib/devices/tv/data.ts';
import { scenesApi, type Scene } from './api.ts';

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

/** A dimmed lamp for a film: a fifth of its brightness. */
const DIM = 20;

/**
 * The three scenes offered when there is none, filled from the house's devices: « Je pars »
 * (every lamp and plug off, the TV and the air conditioner off, every shutter closed),
 * « Nuit » (every lamp off, every shutter closed), « Film » (the TV on, the box's first app,
 * a lamp dimmed).
 */
export function templates(s: Sources): Scene[] {
	const lampsOff: Action[] = [
		...s.lamps.map((l): Action => ({ action: 'zigbee_power', lamp: l.id, state: 'off' })),
		...s.hueLamps.map((l): Action => ({ action: 'hue_power', lamp: l.id, state: 'off' }))
	];
	const plugsOff: Action[] = s.plugs.map((p) => ({ action: 'meross_power', device: p.id, state: 'off' }));
	const acOff: Action[] = s.hosts[0] ? [{ action: 'climate_off', host: s.hosts[0].host }] : [];
	const closed: Action[] = s.covers.map((c) => ({ action: 'cover', cover: c.id, command: 'close' }));
	const dim: Action[] = s.lamps[0]
		? [{ action: 'zigbee_brightness', lamp: s.lamps[0].id, brightness: DIM }]
		: s.hueLamps[0]
			? [{ action: 'hue_brightness', lamp: s.hueLamps[0].id, brightness: DIM }]
			: [];
	const film: Action[] = [
		{ action: 'tv_power', state: 'on', switchToBox: true },
		...(s.apps[0] ? [{ action: 'androidtv_app', package: s.apps[0].package } as Action] : []),
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

/** How a run went, said once: every action done, or which ones failed, each named with the
 * devices' names (`s`) and why. */
function sayRun(scene: Pick<Scene, 'name'>, r: RunResponse, s: Sources) {
	const told = r.results.map((x) => outcome(x, s));
	sayOutcome(
		{
			done: told.filter((o) => o.ok).map((o) => o.what),
			failed: told.filter((o) => !o.ok).map((o) => m.action_failed({ action: o.what, reason: o.why }))
		},
		(count) => m.scenes_done({ name: scene.name, count }),
		(o) => m.scenes_failed({ name: scene.name, count: o.failed.length, total: r.results.length, details: facts(...o.failed) })
	);
}

/** Runs `scene` through the caller's gesture (keyed by its id), says how it went (naming the
 * devices of `sources`), then reads again what it may have changed. */
export function runScene(g: Gesture, scene: Scene, sources: Sources) {
	return g.committed(
		() => scenesApi.run(scene.id),
		async (r) => {
			sayRun(scene, r, sources);
			await Promise.all([HUE, ZIGBEE, MEROSS, SHUTTERS, TV, BOX].map((k) => refresh(k)));
		},
		scene.id
	);
}
