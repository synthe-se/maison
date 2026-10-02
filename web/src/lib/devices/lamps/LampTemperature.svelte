<script lang="ts">
	// The colour-temperature slider, Hue or Zigbee: warm on the left, cool on the right.
	import { m } from '#lib/paraglide/messages.js';
	import { refresh } from '#lib/live.svelte.ts';
	import Range from '#lib/components/Range.svelte';
	import type { Lamp, LampDriver } from './lamp.ts';

	let { lamp, driver }: { lamp: Lamp & { temperature: number }; driver: LampDriver } = $props();
</script>

<Range
	live
	near={2}
	label={m.lamps_temperature()}
	value={lamp.temperature}
	valueText={driver.temperatureText}
	send={(v) => driver.temperature(lamp.id, v)}
	settle={() => refresh(driver.key)}
	disabled={!lamp.reachable || !lamp.isOn}
	ends={[m.lamps_warm(), m.lamps_cool()]}
/>
