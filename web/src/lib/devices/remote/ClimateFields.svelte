<script lang="ts">
	// A « climate toggle » action's fields: the blaster, then the AC's settings as pickers when
	// the order is one they can say; one they cannot (an absolute stop time…) is kept as text,
	// never destroyed.
	import { m } from '#lib/paraglide/messages.js';
	import { options } from '#lib/options.ts';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import {
		buildClimateCommand,
		CLIMATE_FANS,
		CLIMATE_MODES,
		CLIMATE_VANES,
		parseClimateCommand,
		TEMP_MAX_C,
		TEMP_MIN_C,
		type ClimateSettings
	} from '#lib/devices/climate/command.ts';
	import { degrees, FAN_LABEL, MODE_LABEL, VANE_LABEL } from '#lib/devices/climate/labels.ts';
	import type { IrAction } from './api.ts';

	type Toggle = Extract<IrAction, { action: 'climate_toggle' }>;
	interface Props {
		action: Toggle;
		hostOptions: { value: string; label: string }[];
		onchange: (a: IrAction) => void;
	}
	let { action, hostOptions, onchange }: Props = $props();
	const id = $props.id();

	const climate = $derived(parseClimateCommand(action.onCommand));
	function set(patch: Partial<ClimateSettings>) {
		if (climate) onchange({ ...action, onCommand: buildClimateCommand({ ...climate, ...patch }) });
	}
</script>

<p class="hint">{m.remote_fields_climate_hint()}</p>
<Select
	label={m.remote_fields_host()}
	value={action.host}
	options={hostOptions}
	placeholder={m.remote_fields_host_placeholder()}
	onchange={(host) => onchange({ ...action, host })}
/>
{#if climate}
	<div class="three">
		<Select label={m.climate_mode()} value={climate.mode} options={options(CLIMATE_MODES, MODE_LABEL)} onchange={(mode) => set({ mode })} />
		<Select label={m.climate_fan()} value={climate.fan} options={options(CLIMATE_FANS, FAN_LABEL)} onchange={(fan) => set({ fan })} />
		<Select
			label={m.climate_vertical_vane()}
			value={climate.vane}
			options={options(CLIMATE_VANES, VANE_LABEL)}
			onchange={(vane) => set({ vane })}
		/>
	</div>
	<Range
		label={m.climate_temperature()}
		value={climate.temperature}
		min={TEMP_MIN_C}
		max={TEMP_MAX_C}
		valueText={degrees}
		oncommit={(temperature) => set({ temperature })}
	/>
{:else}
	<div class="field">
		<label for="{id}-raw">{m.remote_fields_climate_raw_command()}</label>
		<input
			id="{id}-raw"
			class="mono"
			value={action.onCommand}
			autocomplete="off"
			spellcheck="false"
			oninput={(e) => onchange({ ...action, onCommand: e.currentTarget.value })}
		/>
	</div>
{/if}

<style>
	.three {
		display: grid;
		gap: var(--s-3);
		grid-template-columns: repeat(auto-fit, minmax(min(9rem, 100%), 1fr));
	}
	.mono {
		font-family: var(--font-mono);
	}
</style>
