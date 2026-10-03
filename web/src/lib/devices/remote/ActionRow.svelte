<script lang="ts">
	// One action of a binding: its type, its fields, and the buttons that move or remove it
	// (buttons, so the order can be changed from the keyboard). An action type the configurator
	// does not edit is shown by its summary and carried through untouched.
	import { m } from '#lib/paraglide/messages.js';
	import type { IrAction, IrSwitchState } from '#lib/api.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import {
		buildClimateCommand,
		parseClimateCommand,
		TEMP_MAX_C,
		TEMP_MIN_C,
		type ClimateSettings
	} from '#lib/devices/climate/command.ts';
	import { degrees, fanOptions, modeOptions, vaneOptions } from '#lib/devices/climate/labels.ts';
	import {
		ACTION_TYPES,
		BRIGHTNESS_MAX,
		defaultAction,
		isEditable,
		NABAZTAG_PRESETS,
		summarize,
		SWITCH_STATES,
		type ActionType,
		type Sources
	} from './actions.ts';

	interface Props {
		action: IrAction;
		/** 0-based place in the list. */
		index: number;
		count: number;
		sources: Sources;
		onchange: (a: IrAction) => void;
		onmove: (direction: -1 | 1) => void;
		onremove: () => void;
	}
	let { action, index, count, sources, onchange, onmove, onremove }: Props = $props();
	const id = $props.id();
	const n = $derived(index + 1);

	const typeOptions = (Object.keys(ACTION_TYPES) as ActionType[]).map((t) => ({ value: t, label: ACTION_TYPES[t]() }));
	const stateOptions = (Object.keys(SWITCH_STATES) as IrSwitchState[]).map((s) => ({ value: s, label: SWITCH_STATES[s]() }));

	const lampOptions = $derived(sources.lamps.map((l) => ({ value: l.id, label: l.name })));
	const plugOptions = $derived(sources.plugs.map((p) => ({ value: p.id, label: `${p.name} (${p.ip})` })));
	const hostOptions = $derived(sources.hosts.map((h) => ({ value: h.host, label: `${h.name} (${h.host})` })));
	const codeOptions = $derived(sources.codes.map((c) => ({ value: c.id, label: c.name })));

	const climate = $derived(action.action === 'climate_toggle' ? parseClimateCommand(action.on_command) : null);
	function setClimate(patch: Partial<ClimateSettings>) {
		if (action.action !== 'climate_toggle' || !climate) return;
		onchange({ ...action, on_command: buildClimateCommand({ ...climate, ...patch }) });
	}
</script>

<li class="action" aria-labelledby="{id}-n">
	<div class="head">
		<span class="n" id="{id}-n"><span class="sr-only">{m.remote_action_number({ n })}</span><span aria-hidden="true">{n}</span></span>
		<div class="type">
			{#if isEditable(action)}
				<Select
					label={m.remote_action_type()}
					value={action.action}
					options={typeOptions}
					onchange={(t) => t !== action.action && onchange(defaultAction(t, sources))}
				/>
			{:else}
				<p>{summarize(action, sources)}</p>
				<p class="hint">{m.remote_other_hint()}</p>
			{/if}
		</div>
		<div class="order">
			<button type="button" class="icon-btn" id="{id}-up" disabled={index === 0} aria-label={m.remote_move_up({ n })} onclick={() => onmove(-1)}>
				<Icon name="chevron-up" />
			</button>
			<button
				type="button"
				class="icon-btn"
				id="{id}-down"
				disabled={index === count - 1}
				aria-label={m.remote_move_down({ n })}
				onclick={() => onmove(1)}
			>
				<Icon name="chevron-down" />
			</button>
			<button type="button" class="icon-btn danger" aria-label={m.remote_remove_action({ n })} onclick={onremove}>
				<Icon name="trash" />
			</button>
		</div>
	</div>

	{#if action.action === 'nabaztag'}
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
			<p class="help" id="{id}-cmd-hint">{m.remote_fields_command_hint()}</p>
			<div class="actions" role="group" aria-labelledby="{id}-presets">
				<span class="help" id="{id}-presets">{m.remote_fields_presets()}</span>
				{#each NABAZTAG_PRESETS as preset (preset)}
					<button type="button" class="pill-btn mono" onclick={() => onchange({ ...action, command: preset })}>{preset}</button>
				{/each}
			</div>
		</div>
	{:else if action.action === 'zigbee_power'}
		<div class="grid">
			<Select
				label={m.remote_fields_lamp()}
				value={action.lamp}
				options={lampOptions} placeholder={m.remote_fields_lamp_placeholder()}
				onchange={(lamp) => onchange({ ...action, lamp })}
			/>
			<Select label={m.remote_fields_state()} value={action.state} options={stateOptions} onchange={(state) => onchange({ ...action, state })} />
		</div>
	{:else if action.action === 'zigbee_brightness'}
		<Select
			label={m.remote_fields_lamp()}
			value={action.lamp}
			options={lampOptions} placeholder={m.remote_fields_lamp_placeholder()}
			onchange={(lamp) => onchange({ ...action, lamp })}
		/>
		<Range
			label={m.lamps_brightness()}
			value={action.brightness}
			max={BRIGHTNESS_MAX}
			valueText={(v) => m.remote_brightness_value({ value: v })}
			oncommit={(brightness) => onchange({ ...action, brightness })}
		/>
	{:else if action.action === 'broadlink_code'}
		<div class="grid">
			<Select
				label={m.remote_fields_host()}
				value={action.host}
				options={hostOptions} placeholder={m.remote_fields_host_placeholder()}
				onchange={(host) => onchange({ ...action, host })}
			/>
			<Select
				label={m.remote_fields_code()}
				value={action.code_id}
				options={codeOptions} placeholder={m.remote_fields_code_placeholder()}
				onchange={(code_id) => onchange({ ...action, code_id })}
			/>
		</div>
	{:else if action.action === 'meross_power'}
		<div class="grid">
			<Select
				label={m.remote_fields_device()}
				value={action.device}
				options={plugOptions} placeholder={m.remote_fields_device_placeholder()}
				onchange={(device) => onchange({ ...action, device })}
			/>
			<Select label={m.remote_fields_state()} value={action.state} options={stateOptions} onchange={(state) => onchange({ ...action, state })} />
		</div>
	{:else if action.action === 'climate_toggle'}
		<p class="hint">{m.remote_fields_climate_hint()}</p>
		<Select
			label={m.remote_fields_host()}
			value={action.host}
			options={hostOptions} placeholder={m.remote_fields_host_placeholder()}
			onchange={(host) => onchange({ ...action, host })}
		/>
		{#if climate}
			<div class="grid three">
				<Select label={m.climate_mode()} value={climate.mode} options={modeOptions()} onchange={(mode) => setClimate({ mode })} />
				<Select label={m.climate_fan()} value={climate.fan} options={fanOptions()} onchange={(fan) => setClimate({ fan })} />
				<Select label={m.climate_vertical_vane()} value={climate.vane} options={vaneOptions()} onchange={(vane) => setClimate({ vane })} />
			</div>
			<Range
				label={m.climate_temperature()}
				value={climate.temperature}
				min={TEMP_MIN_C}
				max={TEMP_MAX_C}
				valueText={degrees}
				oncommit={(temperature) => setClimate({ temperature })}
			/>
		{:else}
			<!-- a command the pickers cannot represent (absolute stop time…): kept as text, not destroyed -->
			<div class="field">
				<label for="{id}-raw">{m.remote_fields_climate_raw_command()}</label>
				<input
					id="{id}-raw"
					class="mono"
					value={action.on_command}
					autocomplete="off"
					spellcheck="false"
					oninput={(e) => onchange({ ...action, on_command: e.currentTarget.value })}
				/>
			</div>
		{/if}
	{/if}
</li>

<style>
	.action { display: grid; gap: var(--s-3); padding: var(--s-3); border: 1px solid var(--line); border-radius: var(--radius-m); background: var(--surface); }
	.head { display: flex; align-items: flex-end; gap: var(--s-2); flex-wrap: wrap; }
	.n {
		flex: none; display: grid; place-items: center; width: var(--control-h-xs); height: var(--control-h);
		font: var(--t-label); color: var(--ink-muted); font-variant-numeric: tabular-nums;
	}
	.type { flex: 1 1 12rem; min-width: 0; }
	.type p { margin: 0; }
	.order { display: flex; gap: var(--s-1); margin-left: auto; }
	.order .icon-btn { width: var(--control-h); height: var(--control-h); }
	.danger { color: var(--status-down-text); }
	.grid { display: grid; gap: var(--s-3); grid-template-columns: repeat(auto-fit, minmax(min(12rem, 100%), 1fr)); }
	.grid.three { grid-template-columns: repeat(auto-fit, minmax(min(9rem, 100%), 1fr)); }
	.mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
</style>
