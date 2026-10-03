<script lang="ts">
	// A rabbit action's field: its command, typed (checked by the backend's allow-list), or one
	// of the presets.
	import { m } from '#lib/paraglide/messages.js';
	import type { IrAction } from './api.ts';
	import { NABAZTAG_PRESETS } from './actions.ts';

	type Rabbit = Extract<IrAction, { action: 'nabaztag' }>;
	let { action, onchange }: { action: Rabbit; onchange: (a: IrAction) => void } = $props();
	const id = $props.id();
</script>

<div class="field">
	<label for="{id}-cmd">{m.remote_fields_command()}</label>
	<input
		id="{id}-cmd"
		value={action.command}
		oninput={(e) => onchange({ ...action, command: e.currentTarget.value })}
		placeholder={m.remote_fields_command_placeholder()}
		autocomplete="off"
		aria-describedby="{id}-cmd-hint"
	/>
	<p class="hint" id="{id}-cmd-hint">{m.remote_fields_command_hint()}</p>
	<div class="actions" role="group" aria-labelledby="{id}-presets">
		<span class="hint" id="{id}-presets">{m.remote_fields_presets()}</span>
		{#each NABAZTAG_PRESETS as preset (preset)}
			<button type="button" class="pill-btn mono" onclick={() => onchange({ ...action, command: preset })}>{preset}</button>
		{/each}
	</div>
</div>

<style>
	.mono {
		font-family: var(--font-mono);
	}
</style>
