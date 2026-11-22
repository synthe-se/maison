<script lang="ts">
	// A lamp action's fields, Zigbee or Hue alike: the lamp (from its family's list), then its
	// state (on, off, toggle) or its brightness, a percentage.
	import { m } from '#lib/paraglide/messages.js';
	import { percent } from '#lib/i18n.svelte.ts';
	import { options } from '#lib/options.ts';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import type { Action, SwitchState } from './api.ts';
	import { BRIGHTNESS, SWITCH_STATES, type Sources } from './actions.ts';

	type LampAction = Extract<Action, { action: 'zigbee_power' | 'zigbee_brightness' | 'hue_power' | 'hue_brightness' }>;
	let { action, sources, onchange }: { action: LampAction; sources: Sources; onchange: (a: Action) => void } = $props();

	const STATES = Object.keys(SWITCH_STATES) as SwitchState[];
	const zigbee = $derived(action.action.startsWith('zigbee'));
	const lamps = $derived((zigbee ? sources.lamps : sources.hueLamps).map((l) => ({ value: l.id, label: l.name })));
</script>

<div class="field-grid">
	<Select
		label={m.lamps_lamp()}
		value={action.lamp}
		options={lamps}
		placeholder={m.action_fields_lamp_placeholder()}
		onchange={(lamp) => onchange({ ...action, lamp })}
	/>
	{#if action.action === 'zigbee_power' || action.action === 'hue_power'}
		{@const a = action}
		<Select
			label={m.action_fields_state()}
			value={a.state}
			options={options(STATES, SWITCH_STATES)}
			onchange={(state) => onchange({ ...a, state })}
		/>
	{/if}
</div>
{#if action.action === 'zigbee_brightness' || action.action === 'hue_brightness'}
	{@const a = action}
	<Range
		label={m.lamps_brightness()}
		value={a.brightness}
		min={BRIGHTNESS.min}
		max={BRIGHTNESS.max}
		valueText={(v) => percent(v / 100)}
		oncommit={(brightness) => onchange({ ...a, brightness })}
	/>
{/if}

<style>
</style>
