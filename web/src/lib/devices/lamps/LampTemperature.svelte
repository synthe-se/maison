<script lang="ts">
	// The color-temperature slider, Hue or Zigbee: warm on the left, cool on the right. A lamp
	// that is off or out of reach keeps it in place, not operable, saying why under it.
	import { m } from '#lib/paraglide/messages.js';
	import { refresh } from '#lib/live.svelte.ts';
	import Range from '#lib/components/Range.svelte';
	import { lampState, type Lamp, type LampDriver } from './lamp.ts';

	let { lamp, driver }: { lamp: Lamp & { temperature: number }; driver: LampDriver } = $props();
	const id = $props.id();
	const why = $derived(!lamp.reachable ? lampState(lamp) : !lamp.isOn ? m.lamps_off_reason() : '');
</script>

<Range
	live
	near={2}
	label={m.lamps_temperature()}
	value={lamp.temperature}
	valueText={driver.temperatureText}
	send={(v) => driver.temperature(lamp.id, v)}
	settle={() => refresh(driver.key)}
	reason={why ? `${id}-why` : undefined}
	ends={[m.lamps_warm(), m.lamps_cool()]}
/>
{#if why}<p class="hint" id="{id}-why">{why}</p>{/if}
