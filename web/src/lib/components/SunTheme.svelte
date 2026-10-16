<script lang="ts">
	// The theme by the house's sun: light from sunrise to sunset, dark the rest (the backend says
	// the side). Mounted by the layout while the theme follows the sun (chosen, or a wall screen's
	// own default): signed in, from the house's place, read again just after each sunrise and
	// sunset; signed out (a screen waiting to be paired), from the side alone, every ten minutes.
	// It says the side in `ui.sun`, which the layout applies, and keeps it (`lastSide`) for the
	// next first paint and the next wait; left, it stops saying it (the layout forgets the kept
	// side when the theme stops following the sun).
	import { live } from '#lib/live.svelte.ts';
	import { place } from '#lib/devices/shutters/data.ts';
	import { untilSun } from '#lib/sun/api.ts';
	import { SUN_SIGNED_OUT_MS } from '#lib/paces.ts';
	import { lastSide, sideNow } from '#lib/sun/data.ts';
	import { ui } from '#lib/ui.svelte.ts';

	interface Props {
		/** Someone is in: the house's place, with its hours. */
		house: boolean;
	}
	let { house }: Props = $props();

	// one source or the other for the whole of this mount: the layout keys it by the mode, so a
	// change of session mounts a new one
	// svelte-ignore state_referenced_locally
	const fromHouse = house ? live(place) : null;
	// svelte-ignore state_referenced_locally
	const fromOutside = house ? null : live(sideNow);

	$effect(() => {
		// no answer yet: undefined (the layout keeps the side last said); an answer that says
		// nothing (no place set): null, the system's theme at once
		if ((fromHouse ?? fromOutside)?.data === undefined) return void (ui.sun = undefined);
		const sun = fromHouse?.data?.sun;
		const side = (fromHouse ? sun?.side : fromOutside?.data?.side) ?? null;
		ui.sun = side;
		if (side) lastSide.set({ side, untilMs: Date.now() + (fromHouse ? untilSun(sun) : SUN_SIGNED_OUT_MS) });
		else lastSide.set(undefined);
	});
	$effect(() => () => void (ui.sun = undefined));
</script>
