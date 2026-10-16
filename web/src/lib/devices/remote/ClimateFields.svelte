<script lang="ts">
	// A « climate toggle » action's fields: the blaster, then the AC's settings as pickers when
	// the order is one they can say; one they cannot (an absolute stop time…) is kept as text,
	// never destroyed.
	import { m } from '#lib/paraglide/messages.js';
	import Select from '#lib/components/Select.svelte';
	import { buildClimateCommand, parseClimateCommand } from '#lib/devices/climate/command.ts';
	import SettingsFields from '#lib/devices/climate/SettingsFields.svelte';
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
	<SettingsFields settings={climate} onchange={(s) => onchange({ ...action, onCommand: buildClimateCommand(s) })} />
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
	.mono {
		font-family: var(--font-mono);
	}
</style>
