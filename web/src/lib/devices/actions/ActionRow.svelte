<script lang="ts">
	// One action of a scene: its type, its fields, and the buttons that move or remove it
	// (buttons, so the order can be changed from the keyboard). An action type the editor
	// does not edit is shown by its summary and carried through untouched.
	import { m } from '#lib/paraglide/messages.js';
	import { options } from '#lib/options.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import { unavailable } from '#lib/gesture.svelte.ts';
	import type { Action, CoverCommand, SwitchState } from './api.ts';
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
		action: Action;
		/** 0-based place in the list. */
		index: number;
		count: number;
		sources: Sources;
		onchange: (a: Action) => void;
		onmove: (direction: -1 | 1) => void;
		onremove: () => void;
	}
	let { action, index, count, sources, onchange, onmove, onremove }: Props = $props();
	const TYPES = Object.keys(ACTION_TYPES) as ActionType[];
	const id = $props.id();
	const n = $derived(index + 1);

	const STATES = Object.keys(SWITCH_STATES) as SwitchState[];
	const COVER = Object.keys(COVER_COMMANDS) as CoverCommand[];
	const TV_STATES = ['on', 'off'] as const;

	const coverOptions = $derived(sources.covers.map((c) => ({ value: c.id, label: c.name })));
	const plugOptions = $derived(sources.plugs.map((p) => ({ value: p.id, label: `${p.name} (${p.ip})` })));
	const hostOptions = $derived(sources.hosts.map((h) => ({ value: h.host, label: `${h.name} (${h.host})` })));
	const codeOptions = $derived(sources.codes.map((c) => ({ value: c.id, label: c.name })));
	const appOptions = $derived(sources.apps.map((a) => ({ value: a.package, label: a.label })));
</script>

<li class="action" aria-labelledby="{id}-n">
	<div class="head">
		<span class="n" id="{id}-n"><span class="sr-only">{m.action_number({ n })}</span><span aria-hidden="true">{n}</span></span>
		<div class="type field-grow">
			{#if isEditable(action)}
				<Select
					label={m.action_type()}
					value={action.action}
					options={options(TYPES, ACTION_TYPES)}
					onchange={(t) => t !== action.action && onchange(defaultAction(t, sources))}
				/>
			{:else}
				<p>{summarize(action, sources)}</p>
				<p class="hint">{m.action_other_hint()}</p>
			{/if}
		</div>
		<div class="order">
			<button
				type="button"
				class="icon-btn control"
				id="{id}-up"
				{...unavailable(index === 0 && `${id}-first`)}
				aria-label={m.action_move_up({ n })}
				onclick={() => index > 0 && onmove(-1)}
			>
				<Icon name="chevron-up" />
			</button>
			<button
				type="button"
				class="icon-btn control"
				id="{id}-down"
				{...unavailable(index === count - 1 && `${id}-last`)}
				aria-label={m.action_move_down({ n })}
				onclick={() => index < count - 1 && onmove(1)}
			>
				<Icon name="chevron-down" />
			</button>
			<button type="button" class="icon-btn danger" aria-label={m.action_remove({ n })} onclick={onremove}>
				<Icon name="trash" />
			</button>
			<span class="sr-only" id="{id}-first">{m.action_move_first()}</span>
			<span class="sr-only" id="{id}-last">{m.action_move_last()}</span>
		</div>
	</div>

	{#if action.action === 'nabaztag'}
		<NabaztagFields {action} {onchange} />
	{:else if action.action === 'zigbee_power' || action.action === 'zigbee_brightness' || action.action === 'hue_power' || action.action === 'hue_brightness'}
		<LampFields {action} {sources} {onchange} />
	{:else if action.action === 'cover'}
		<div class="field-grid">
			<Select
				label={m.action_fields_cover()}
				value={action.cover}
				options={coverOptions}
				placeholder={m.action_fields_cover_placeholder()}
				onchange={(cover) => onchange({ ...action, cover })}
			/>
			<Select
				label={m.action_fields_command()}
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
		{#if action.action === 'climate_on'}<p class="hint">{m.action_fields_climate_on_hint()}</p>{/if}
		<Select
			label={m.action_fields_host()}
			value={action.host}
			options={hostOptions}
			placeholder={m.action_fields_host_placeholder()}
			onchange={(host) => onchange({ ...action, host })}
		/>
	{:else if action.action === 'broadlink_code'}
		<div class="field-grid">
			<Select
				label={m.action_fields_host()}
				value={action.host}
				options={hostOptions}
				placeholder={m.action_fields_host_placeholder()}
				onchange={(host) => onchange({ ...action, host })}
			/>
			<Select
				label={m.action_fields_code()}
				value={action.codeId}
				options={codeOptions}
				placeholder={m.action_fields_code_placeholder()}
				onchange={(codeId) => onchange({ ...action, codeId })}
			/>
		</div>
	{:else if action.action === 'meross_power'}
		<div class="field-grid">
			<Select
				label={m.action_fields_device()}
				value={action.device}
				options={plugOptions}
				placeholder={m.action_fields_device_placeholder()}
				onchange={(device) => onchange({ ...action, device })}
			/>
			<Select
				label={m.action_fields_state()}
				value={action.state}
				options={options(STATES, SWITCH_STATES)}
				onchange={(state) => onchange({ ...action, state })}
			/>
		</div>
	{:else if action.action === 'tv_power'}
		<Select
			label={m.action_fields_state()}
			value={action.state}
			options={options(TV_STATES, SWITCH_STATES)}
			onchange={(state) => onchange({ ...action, state })}
		/>
	{:else if action.action === 'androidtv_app'}
		<Select
			label={m.action_fields_app()}
			value={action.package}
			options={appOptions}
			placeholder={m.action_fields_app_placeholder()}
			onchange={(pkg) => onchange({ ...action, package: pkg })}
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
	.type p {
		margin: 0;
	}
	.order {
		display: flex;
		gap: var(--s-1);
		margin-left: auto;
	}
	.danger {
		color: var(--status-down-text);
	}
</style>
