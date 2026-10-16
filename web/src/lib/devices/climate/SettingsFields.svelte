<script lang="ts">
	// The air conditioner's settings, said once (the dashboard's form, and a remote key's or a
	// scene's « climate toggle » action): mode, fan and vane, the temperature, econo cooling.
	// Econo works in cooling only: it stays in place, unavailable, saying so, in any other mode,
	// and switching to another mode turns it off.
	import { m } from '#lib/paraglide/messages.js';
	import { options } from '#lib/options.ts';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import { CLIMATE_FANS, CLIMATE_MODES, CLIMATE_VANES, TEMP_MAX_C, TEMP_MIN_C, type ClimateSettings } from './command.ts';
	import { FAN_LABEL, MODE_LABEL, VANE_LABEL } from './labels.ts';

	interface Props {
		settings: ClimateSettings;
		onchange: (s: ClimateSettings) => void;
	}
	let { settings, onchange }: Props = $props();
	const id = $props.id();
	const set = (patch: Partial<ClimateSettings>) => onchange({ ...settings, ...patch });
</script>

<div class="three">
	<Select
		label={m.climate_mode()}
		value={settings.mode}
		options={options(CLIMATE_MODES, MODE_LABEL)}
		onchange={(mode) => set({ mode, econo: mode === 'cool' && !!settings.econo })}
	/>
	<Select label={m.climate_fan()} value={settings.fan} options={options(CLIMATE_FANS, FAN_LABEL)} onchange={(fan) => set({ fan })} />
	<Select
		label={m.climate_vertical_vane()}
		value={settings.vane}
		options={options(CLIMATE_VANES, VANE_LABEL)}
		onchange={(vane) => set({ vane })}
	/>
</div>
<Range
	label={m.climate_temperature()}
	value={settings.temperature}
	min={TEMP_MIN_C}
	max={TEMP_MAX_C}
	valueText={(v) => m.climate_degrees_words({ degrees: v })}
	oncommit={(temperature) => set({ temperature })}
/>
<div>
	<Toggle
		label={m.climate_econo_cool()}
		checked={!!settings.econo}
		reason={settings.mode === 'cool' ? undefined : `${id}-econo`}
		onchange={(econo) => set({ econo })}
	/>
	<p class="hint" id="{id}-econo">{m.climate_econo_cool_description()}</p>
</div>

<style>
	.three {
		display: grid;
		gap: var(--s-3);
		grid-template-columns: repeat(auto-fit, minmax(min(9rem, 100%), 1fr));
	}
</style>
