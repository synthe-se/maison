<script lang="ts">
	// The fountain's page: its settings as switches (§ 2, case 2), its levels and counters; a
	// counter reset asks first (§ 6).
	import { m } from '#lib/paraglide/messages.js';
	import { formatMinutes, formatDuration } from '#lib/format.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { options } from '#lib/options.ts';
	import Section from '#lib/components/Section.svelte';
	import SettingRow from '#lib/components/SettingRow.svelte';
	import StatusRow from '#lib/components/StatusRow.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import ToggleGroup from '#lib/components/ToggleGroup.svelte';
	import { fountainApi } from './api.ts';
	import { status, TUYA } from './data.ts';
	import { reread } from './tuya.svelte.ts';

	const WATER: Record<string, () => string> = {
		low: m.device_level_low,
		medium: m.device_level_medium,
		ok: m.fountain_water_level_ok
	};
	/** The device's two eco modes, by the number it takes. */
	const ECO_MODES = ['1', '2'] as const;
	const ECO_LABEL = { '1': m.fountain_mode1, '2': m.fountain_mode2 };

	/** One counter: what it shows, how it is reset and what is said after. */
	const COUNTERS = [
		{
			key: 'water',
			field: 'waterTime',
			icon: 'droplets',
			label: m.fountain_fresh_water,
			button: m.fountain_water_changed,
			title: m.fountain_reset_water_title,
			done: m.fountain_water_counter_reset,
			reset: fountainApi.resetWater
		},
		{
			key: 'filter',
			field: 'filterLife',
			icon: 'funnel',
			label: m.fountain_filter_time,
			button: m.fountain_reset_filter_changed,
			title: m.fountain_reset_filter_title,
			done: m.fountain_filter_counter_reset,
			reset: fountainApi.resetFilter
		},
		{
			key: 'pump',
			field: 'pumpTime',
			icon: 'gauge',
			label: m.fountain_pump_time,
			button: m.fountain_reset_pump_cleaned,
			title: m.fountain_reset_pump_title,
			done: m.fountain_pump_counter_reset,
			reset: fountainApi.resetPump
		}
	] as const;

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const st = live(status('fountain', id));
	const s = $derived(st.data);
	const uvOn = $derived((s?.uvRuntime ?? 0) > 0 || (s?.uv ?? false));
	const g = new Gesture();
	const prefix = $derived(`${TUYA}${id}`);
</script>

<Loaded value={st}>
	<Section title={m.common_settings()} icon="droplets">
		<ul class="settings">
			<SettingRow
				icon="power"
				label={m.fountain_power()}
				checked={s?.power ?? false}
				busy={g.is('power')}
				onchange={(on) =>
					g.run(() => fountainApi.power(id, on), reread(prefix, on ? m.fountain_fountain_on() : m.fountain_fountain_off()), 'power')}
			/>
			<SettingRow
				icon="sun"
				label={m.fountain_uv_sterilization()}
				checked={uvOn}
				busy={g.is('uv')}
				hint={uvOn && s?.uvRuntime ? m.fountain_uv_runtime({ time: formatDuration(s.uvRuntime) }) : undefined}
				onchange={(on) => g.run(() => fountainApi.setUV(id, on), reread(prefix, on ? m.fountain_uv_on() : m.fountain_uv_off()), 'uv')}
			/>
		</ul>
		<div class="block">
			<ToggleGroup
				type="single"
				label={m.fountain_eco_mode()}
				options={options(ECO_MODES, ECO_LABEL)}
				value={s?.ecoMode ? (String(s.ecoMode) as (typeof ECO_MODES)[number]) : undefined}
				busy={g.is('eco')}
				onchange={(mode) => g.run(() => fountainApi.setEcoMode(id, Number(mode)), reread(prefix, m.fountain_eco_mode_on({ mode })), 'eco')}
			/>
			<p class="hint">{m.fountain_eco_mode_description()}</p>
		</div>
	</Section>

	<Section title={m.fountain_status_maintenance()}>
		<dl class="facts">
			<StatusRow
				icon="droplets"
				label={m.fountain_water_level()}
				value={s?.waterLevel ? (WATER[s.waterLevel]?.() ?? s.waterLevel) : m.common_unknown()}
				warn={s?.waterLevel === 'low'}
			/>
			{#each COUNTERS as c (c.key)}
				{#if s?.[c.field] !== undefined}
					<StatusRow icon={c.icon} label={c.label()} value={formatMinutes(s[c.field] ?? 0)} />
				{/if}
			{/each}
		</dl>
		{#if s?.waterLevel === 'low'}<p class="warn-text">{m.fountain_add_water()}</p>{/if}
		<div class="resets">
			{#each COUNTERS as c (c.key)}
				<ConfirmDialog
					label={c.button()}
					icon="refresh-cw"
					title={c.title()}
					description={m.fountain_reset_description()}
					action={m.device_reset()}
					busy={g.is(c.key)}
					onconfirm={() => g.run(() => c.reset(id), reread(prefix, c.done()), c.key)}
				/>
			{/each}
		</div>
	</Section>
</Loaded>

<style>
	.resets {
		display: grid;
		gap: var(--s-2);
	}
</style>
