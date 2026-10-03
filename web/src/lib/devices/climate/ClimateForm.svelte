<script lang="ts">
	// The air conditioner's settings form, unfolded in its tile (TileSettings): mode,
	// temperature, fan, vane, an off timer, econo cooling, the infrared order it makes (folded,
	// for whoever debugs), and « Envoyer ». The form belongs to the tile (`form`, bound): its
	// order is what « Allumer » sends, settings open or not.
	import { m } from '#lib/paraglide/messages.js';
	import { formatMinutes } from '#lib/format.ts';
	import { options } from '#lib/options.ts';
	import { pending, unavailable } from '#lib/gesture.svelte.ts';
	import Disclosure from '#lib/components/Disclosure.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Select from '#lib/components/Select.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import ToggleGroup from '#lib/components/ToggleGroup.svelte';
	import { CLIMATE_FANS, CLIMATE_MODES, CLIMATE_VANES, STOP_IN_STEP_MIN, TEMP_MAX_C, TEMP_MIN_C } from './command.ts';
	import { FAN_LABEL, MODE_LABEL, VANE_LABEL } from './labels.ts';
	import { INITIAL, type ClimateForm } from './form.ts';

	interface Props {
		form: ClimateForm;
		/** The order the form makes. */
		command: string;
		/** The blaster's address, once found. */
		host: string | undefined;
		/** Whether an order travels; `why`: the id of what says no blaster is found. */
		busy: boolean;
		why: string | undefined;
		send: (command: string) => unknown;
	}
	let { form = $bindable(), command, host, busy, why, send }: Props = $props();
	const id = $props.id();

	/** Sleep-timer presets, like the remote's 1 h / 3 h buttons (10-minute ticks). */
	const STOP_AFTER_CHOICES = [30, 60, 120, 180, 300, 480, 720];
	const TIMER = ['none', 'stop'] as const;
	const TIMER_LABEL = { none: m.climate_timer_modes_none, stop: m.climate_timer_modes_stop };

	// a restored timer may carry any multiple of 10 minutes: keep it in the list
	const stopOptions = $derived(
		[...new Set([...STOP_AFTER_CHOICES, form.stopAfter])]
			.filter((n) => Number.isInteger(n) && n > 0 && n % STOP_IN_STEP_MIN === 0)
			.toSorted((a, b) => a - b)
			.map((n) => ({ value: String(n), label: formatMinutes(n) }))
	);
</script>

{#if host}<p class="hint">{m.climate_remote_connected({ host })}</p>{/if}
<Select
	label={m.climate_mode()}
	value={form.mode}
	options={options(CLIMATE_MODES, MODE_LABEL)}
	onchange={(v) => (form = { ...form, mode: v, econo: v === 'cool' && form.econo })}
/>
<Range
	label={m.climate_temperature()}
	value={form.temperature}
	min={TEMP_MIN_C}
	max={TEMP_MAX_C}
	valueText={(v) => m.climate_degrees_words({ degrees: v })}
	oncommit={(v) => (form.temperature = v)}
/>
<Select label={m.climate_fan()} value={form.fan} options={options(CLIMATE_FANS, FAN_LABEL)} onchange={(v) => (form.fan = v)} />
<Select
	label={m.climate_vertical_vane()}
	value={form.vane}
	options={options(CLIMATE_VANES, VANE_LABEL)}
	onchange={(v) => (form.vane = v)}
/>

<ToggleGroup
	type="single"
	label={m.climate_timer_mode()}
	options={options(TIMER, TIMER_LABEL)}
	value={form.timer ? 'stop' : 'none'}
	onchange={(v) => (form.timer = v === 'stop')}
/>
{#if form.timer}
	<Select
		label={m.climate_stop_after()}
		value={String(form.stopAfter)}
		options={stopOptions}
		onchange={(v) => (form.stopAfter = Number(v))}
	/>
{/if}

<div>
	<Toggle
		label={m.climate_econo_cool()}
		checked={form.econo}
		reason={form.mode === 'cool' ? undefined : `${id}-econo`}
		onchange={(v) => (form.econo = v)}
	/>
	<p class="hint" id="{id}-econo">{m.climate_econo_cool_description()}</p>
</div>

<!-- the infrared order itself: for whoever debugs, folded -->
<Disclosure label={m.climate_generated_command()}>
	<code>{command}</code>
</Disclosure>

<div class="actions">
	<button class="btn primary" {...why ? unavailable(why) : pending(busy)} onclick={() => !why && !busy && send(command)}>
		<Icon name="power" {busy} />{m.climate_send_structured_command()}
	</button>
	<button class="btn" onclick={() => (form = { ...INITIAL })}>{m.climate_reset()}</button>
</div>

<style>
	code {
		overflow-wrap: anywhere;
		color: var(--ink);
		font: var(--t-secondary);
		font-family: var(--font-mono);
	}
</style>
