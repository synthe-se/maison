<script lang="ts">
	// The theme by the house's sun: light from sunrise to sunset, dark the rest. Mounted by the
	// layout while someone is in and the theme follows the sun (chosen, or a wall screen's
	// default); it reads the house's place (read again just after each sunrise and sunset) and
	// says the side in `ui.sun`, which the layout applies.
	import { live } from '#lib/live.svelte.ts';
	import { place } from '#lib/devices/shutters/data.ts';
	import { sunSide } from '#lib/sun.ts';
	import { ui } from '#lib/ui.svelte.ts';

	const here = live(place);

	$effect(() => {
		ui.sun = sunSide(here.data?.sun);
		return () => void (ui.sun = null);
	});
</script>
