// The Android TV box (MECOOL LEAP-S1), driven the same way from its tile and its page:
// wake/sleep (CEC follows: waking it turns the TV on and takes its input, sleeping it puts the
// TV to standby) and its app shortcuts. Construct during component initialisation.

import { m } from '#lib/paraglide/messages.js';
import { live } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { Command, LIMIT } from '#lib/command.svelte.ts';
import { Gesture } from '#lib/gesture.svelte.ts';
import { CONFIRM, haptic } from '#lib/haptics.ts';
import { androidTvApi, type AndroidApp } from './api.ts';
import { boxApps } from './apps.ts';
import { box as boxData } from './data.ts';
import { boxState } from './words.ts';

export class Box {
	box = live(boxData);
	status = $derived(this.box.data?.status);
	config = $derived(this.box.data?.config);
	configured = $derived(this.status?.configured ?? false);
	reachable = $derived(this.status?.reachable ?? false);
	shortcuts = $derived(boxApps(this.config));
	current = $derived(this.shortcuts.find((a) => a.package === this.status?.currentApp));
	/** Not the group's name (« Android TV » > « Box du salon »). */
	name = $derived(this.status?.model || m.android_tv_box());
	/** Line 2 of the tile, in words. */
	state = $derived(boxState(this.status));
	// waking the box wakes the TV through CEC: the TV's 30 s limit
	awake = new Command(() => this.name, LIMIT.tv);
	/** Launching an app (keyed by package; any launch holds the shortcuts). */
	apps = new Gesture();

	toggle = async (on: boolean) => {
		haptic(CONFIRM);
		const ok = await this.awake.run(on, () => (on ? androidTvApi.wake() : androidTvApi.sleep()));
		if (!ok) return;
		this.box.update((d) => ({ ...d, status: { ...d.status, awake: on } }));
		void this.box.refresh();
	};

	launch = (app: AndroidApp) => {
		if (this.apps.is()) return;
		haptic(CONFIRM);
		return this.apps.run(
			() => androidTvApi.launch(app.package),
			() => {
				ui.toast(m.android_tv_launched({ name: app.label }));
				void this.box.refresh();
			},
			app.package
		);
	};
}
