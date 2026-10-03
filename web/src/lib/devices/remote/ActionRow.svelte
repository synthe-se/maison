<script lang="ts">
	// One action of a binding: its type, its fields, and the buttons that move or remove it
	// (buttons, so the order can be changed from the keyboard). An action type the configurator
	// does not edit is shown by its summary and carried through untouched.
	import { m } from '#lib/paraglide/messages.js';
	import { options } from '#lib/options.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import type { IrAction, IrCoverCommand, IrSwitchState } from './api.ts';
	import ClimateFields from './ClimateFields.svelte';
	import LampFields from './LampFields.svelte';
	import NabaztagFields from './NabaztagFields.svelte';
	import {
		ACTION_TYPES,
		COVER_COMMANDS,
		defaultAction,
		HALF,
		isEditable,
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
		/** The types offered (a scene's actions never hold a scene); all by default. */
		types?: ActionType[];
	}
	let { action, index, count, sources, onchange, onmove, onremove, types = Object.keys(ACTION_TYPES) as ActionType[] }: Props = $props();
	const id = $props.id();
	const n = $derived(index + 1);

	const STATES = Object.keys(SWITCH_STATES) as IrSwitchState[];
	const COVER = Object.keys(COVER_COMMANDS) as IrCoverCommand[];
	const TV_STATES = ['on', 'off'] as const;

	const coverOptions = $derived(sources.covers.map((c) => ({ value: c.id, label: c.name })));
	const plugOptions = $derived(sources.plugs.map((p) => ({ value: p.id, label: `${p.name} (${p.ip})` })));
	const hostOptions = $derived(sources.hosts.map((h) => ({ value: h.host, label: `${h.name} (${h.host})` })));
	const codeOptions = $derived(sources.codes.map((c) => ({ value: c.id, label: c.name })));
	const appOptions = $derived(sources.apps.map((a) => ({ value: a.package, label: a.label })));
	const sceneOptions = $derived(sources.scenes.map((x) => ({ value: x.id, label: x.name })));
</script>

<li class="action" aria-labelledby="{id}-n">
	<div class="head">
		<span class="n" id="{id}-n"><span class="sr-only">{m.remote_action_number({ n })}</span><span aria-hidden="true">{n}</span></span>
		<div class="type">
			{#if isEditable(action)}
				<Select
					label={m.remote_action_type()}
					value={action.action}
					options={options(types, ACTION_TYPES)}
					onchange={(t) => t !== action.action && onchange(defaultAction(t, sources))}
				/>
			{:else}
				<p>{summarize(action, sources)}</p>
				<p class="hint">{m.remote_other_hint()}</p>
			{/if}
		</div>
		<div class="order">
			<button
				type="button"
				class="icon-btn"
				id="{id}-up"
				disabled={index === 0}
				aria-label={m.remote_move_up({ n })}
				onclick={() => onmove(-1)}
			>
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
		<NabaztagFields {action} {onchange} />
	{:else if action.action === 'zigbee_power' || action.action === 'zigbee_brightness' || action.action === 'hue_power' || action.action === 'hue_brightness'}
		<LampFields {action} {sources} {onchange} />
	{:else if action.action === 'cover'}
		<div class="grid">
			<Select
				label={m.remote_fields_cover()}
				value={action.cover}
				options={coverOptions}
				placeholder={m.remote_fields_cover_placeholder()}
				onchange={(cover) => onchange({ ...action, cover })}
			/>
			<Select
				label={m.remote_fields_command()}
				value={action.command}
				options={options(COVER, COVER_COMMANDS)}
				onchange={(command) => {
					// a position travels with its command only (the backend refuses it elsewhere)
					const { position: _, ...rest } = action;
					onchange(command === 'position' ? { ...rest, command, position: action.position ?? HALF } : { ...rest, command });
				}}
			/>
		</div>
		{#if action.command === 'position'}
			<Range
				label={m.shutters_position()}
				value={action.position ?? HALF}
				valueText={(v) => m.shutters_open_percent({ percent: v })}
				oncommit={(position) => onchange({ ...action, position })}
			/>
		{/if}
	{:else if action.action === 'climate_off' || action.action === 'climate_on'}
		{#if action.action === 'climate_on'}<p class="hint">{m.remote_fields_climate_on_hint()}</p>{/if}
		<Select
			label={m.remote_fields_host()}
			value={action.host}
			options={hostOptions}
			placeholder={m.remote_fields_host_placeholder()}
			onchange={(host) => onchange({ ...action, host })}
		/>
	{:else if action.action === 'broadlink_code'}
		<div class="grid">
			<Select
				label={m.remote_fields_host()}
				value={action.host}
				options={hostOptions}
				placeholder={m.remote_fields_host_placeholder()}
				onchange={(host) => onchange({ ...action, host })}
			/>
			<Select
				label={m.remote_fields_code()}
				value={action.codeId}
				options={codeOptions}
				placeholder={m.remote_fields_code_placeholder()}
				onchange={(codeId) => onchange({ ...action, codeId })}
			/>
		</div>
	{:else if action.action === 'meross_power'}
		<div class="grid">
			<Select
				label={m.remote_fields_device()}
				value={action.device}
				options={plugOptions}
				placeholder={m.remote_fields_device_placeholder()}
				onchange={(device) => onchange({ ...action, device })}
			/>
			<Select
				label={m.remote_fields_state()}
				value={action.state}
				options={options(STATES, SWITCH_STATES)}
				onchange={(state) => onchange({ ...action, state })}
			/>
		</div>
	{:else if action.action === 'tv_power'}
		<Select
			label={m.remote_fields_state()}
			value={action.state}
			options={options(TV_STATES, SWITCH_STATES)}
			onchange={(state) => onchange({ ...action, state })}
		/>
	{:else if action.action === 'androidtv_app'}
		<Select
			label={m.remote_fields_app()}
			value={action.package}
			options={appOptions}
			placeholder={m.remote_fields_app_placeholder()}
			onchange={(pkg) => onchange({ ...action, package: pkg })}
		/>
	{:else if action.action === 'scene'}
		<Select
			label={m.remote_fields_scene()}
			value={action.scene}
			options={sceneOptions}
			placeholder={m.remote_fields_scene_placeholder()}
			onchange={(scene) => onchange({ ...action, scene })}
		/>
	{:else if action.action === 'climate_toggle'}
		<ClimateFields {action} {hostOptions} {onchange} />
	{/if}
</li>

<style>
	.action {
		display: grid;
		gap: var(--s-3);
		padding: var(--s-3);
		border: 1px solid var(--line);
		border-radius: var(--radius-m);
		background: var(--surface);
	}
	.head {
		display: flex;
		align-items: flex-end;
		gap: var(--s-2);
		flex-wrap: wrap;
	}
	.n {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--control-h-xs);
		height: var(--control-h);
		font: var(--t-label);
		color: var(--ink-muted);
		font-variant-numeric: tabular-nums;
	}
	.type {
		flex: 1 1 12rem;
		min-width: 0;
	}
	.type p {
		margin: 0;
	}
	.order {
		display: flex;
		gap: var(--s-1);
		margin-left: auto;
	}
	.order .icon-btn {
		width: var(--control-h);
		height: var(--control-h);
	}
	.danger {
		color: var(--status-down-text);
	}
	.grid {
		display: grid;
		gap: var(--s-3);
		grid-template-columns: repeat(auto-fit, minmax(min(12rem, 100%), 1fr));
	}
</style>
