<script lang="ts">
	// The fountain's page: its settings as switches (§ 2, case 2), its levels and counters; a
	// counter reset asks first (§ 6).
	import { m } from '#lib/paraglide/messages.js';
	import { fountainApi } from '#lib/api.ts';
	import { formatMinutes, formatDuration } from '#lib/format.ts';
	import Section from './Section.svelte';
	import SettingRow from '#lib/components/SettingRow.svelte';
	import StatusRow from './StatusRow.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import type { FountainStatus } from '#lib/api.ts';
	import Loaded from '#lib/components/Loaded.svelte';
	import { reread, status } from './tuya.svelte.ts';

	const WATER: Record<string, () => string> = {
		low: m.device_level_low,
		medium: m.device_level_medium,
		ok: m.fountain_water_level_ok
	};
	const ECO_MODES = [1, 2] as const;
	const ECO_LABEL = { 1: m.fountain_mode1, 2: m.fountain_mode2 };

	/** One counter: what it shows, how it is reset and what is said after. */
	const COUNTERS = [
		{ key: 'water', field: 'water_time', icon: 'droplets', label: m.fountain_fresh_water, button: m.fountain_water_changed, title: m.fountain_reset_water_title, done: m.fountain_water_counter_reset, reset: fountainApi.resetWater },
		{ key: 'filter', field: 'filter_life', icon: 'funnel', label: m.fountain_filter_time, button: m.fountain_reset_filter_changed, title: m.fountain_reset_filter_title, done: m.fountain_filter_counter_reset, reset: fountainApi.resetFilter },
		{ key: 'pump', field: 'pump_time', icon: 'gauge', label: m.fountain_pump_time, button: m.fountain_reset_pump_cleaned, title: m.fountain_reset_pump_title, done: m.fountain_pump_counter_reset, reset: fountainApi.resetPump }
	] as const;

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const st = status<FountainStatus>('fountain', id);
	const s = $derived(st.data);
	const uvOn = $derived((s?.uv_runtime ?? 0) > 0 || (s?.uv ?? s?.uv_enabled ?? false));
	const g = new Gesture();
	const prefix = $derived(`tuya:${id}`);
</script>

<Loaded value={st}>
	<Section title={m.common_settings()} icon="droplets">
		<ul class="settings">
			<SettingRow
				icon="power"
				label={m.fountain_power()}
				checked={s?.power ?? false}
				pending={g.is('power')}
				onchange={(on) => g.run(() => fountainApi.power(id, on), reread(prefix, on ? m.fountain_fountain_on() : m.fountain_fountain_off()), 'power')}
			/>
			<SettingRow
				icon="sun"
				label={m.fountain_uv_sterilization()}
				checked={uvOn}
				pending={g.is('uv')}
				hint={uvOn && s?.uv_runtime ? m.fountain_uv_runtime({ time: formatDuration(s.uv_runtime) }) : undefined}
				onchange={(on) => g.run(() => fountainApi.setUV(id, on), reread(prefix, on ? m.fountain_uv_on() : m.fountain_uv_off()), 'uv')}
			/>
		</ul>
		<div class="field">
			<span class="label" id="eco-{id}">{m.fountain_eco_mode()}</span>
			<p class="hint">{m.fountain_eco_mode_description()}</p>
			<div class="btn-row segmented" role="group" aria-labelledby="eco-{id}">
				{#each ECO_MODES as mode (mode)}
					<button
						class="btn"
						aria-pressed={s?.eco_mode === mode}
						{...pending(g.is('eco'))}
						onclick={() => g.run(() => fountainApi.setEcoMode(id, mode), reread(prefix, m.fountain_eco_mode_on({ mode })), 'eco')}
					>
						{ECO_LABEL[mode]()}
					</button>
				{/each}
			</div>
		</div>
	</Section>

	<Section title={m.fountain_status_maintenance()}>
		<dl class="facts">
			<StatusRow
				icon="droplets"
				label={m.fountain_water_level()}
				value={s?.water_level ? (WATER[s.water_level]?.() ?? s.water_level) : m.common_unknown()}
				warn={s?.water_level === 'low'}
			/>
			{#each COUNTERS as c (c.key)}
				{#if s?.[c.field] !== undefined}
					<StatusRow icon={c.icon} label={c.label()} value={formatMinutes(s[c.field] ?? 0)} />
				{/if}
			{/each}
		</dl>
		{#if s?.water_level === 'low'}<p class="warn-text">{m.fountain_add_water()}</p>{/if}
		<div class="resets">
			{#each COUNTERS as c (c.key)}
				<ConfirmDialog
					label={c.button()}
					icon="refresh-cw"
					title={c.title()}
					description={m.fountain_reset_description()}
					action={m.device_reset()}
					pending={g.is(c.key)}
					onconfirm={() => g.run(() => c.reset(id), reread(prefix, c.done()), c.key)}
				/>
			{/each}
		</div>
	</Section>
</Loaded>

<style>
	.resets { display: grid; gap: var(--s-2); }

</style>
