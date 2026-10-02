<script lang="ts">
	import { clock, percent } from '#lib/i18n.svelte.ts';
	// The feeder's page: serve now (no dialog: the count is in the button, § 6), what it
	// reports, and its scheduled meals.
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import DeviceTabs from './DeviceTabs.svelte';
	import Section from './Section.svelte';
	import StatusRow from './StatusRow.svelte';
	import MealPlanManager from './MealPlanManager.svelte';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { feed, served, status } from './tuya.svelte.ts';

	/** What the backend parses today, plus what older firmwares reported (shown when present). */
	interface FeederStatus {
		food_level?: string;
		battery_level?: number;
		is_feeding?: boolean;
		error?: string;
		system?: { fault_status?: boolean; powered_by?: string };
	}

	const FOOD: Record<string, () => string> = {
		low: m.device_level_low,
		medium: m.device_level_medium,
		full: m.feeder_food_level_full
	};
	const POWER: Record<string, () => string> = { 'AC Power': m.feeder_power_mains, Battery: m.feeder_power_battery };
	/** Portions in one manual serving. */
	const MAX_PORTIONS = 10;
	const LOW_BATTERY = 20;

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const st = status<FeederStatus>('feeder', id);
	const s = $derived(st.data);

	let portions = $state(1);
	const g = new Gesture();
	const serve = () => feed(g, id, portions);
</script>

<DeviceTabs other={{ label: m.feeder_schedule(), icon: 'calendar' }}>
	{#snippet controls()}
		<Section title={m.feeder_manual_distribution()} icon="utensils">
			<Range
				label={m.feeder_portions()}
				value={portions}
				min={1}
				max={MAX_PORTIONS}
				valueText={(count) => m.feeder_portion({ count })}
				oncommit={(v) => (portions = v)}
				disabled={g.is()}
			/>
			<button class="btn primary wide" disabled={g.is()} aria-busy={g.is()} onclick={serve}>
				{#if g.is()}<Icon name="loader-circle" class="spin" />{m.feeder_distributing()}{:else}<Icon name="utensils" />{m.feeder_distribute({ count: portions })}{/if}
			</button>
			{#if served[id]}<p class="hint">{m.feeder_served_at({ time: clock(served[id]) })}</p>{/if}
		</Section>

		<Section title={m.feeder_feeder_status()}>
			{#if st.loading}
				<p class="hint" role="status">{m.common_loading()}</p>
			{:else if !s}
				<p class="hint">{m.feeder_status_fetch_error()}</p>
			{:else}
				<dl class="facts">
					{#if s.food_level}
						<StatusRow icon="utensils" label={m.feeder_food_level()} value={FOOD[s.food_level]?.() ?? s.food_level} warn={s.food_level === 'low'} />
					{/if}
					{#if s.battery_level !== undefined}
						<StatusRow icon="battery" label={m.feeder_battery()} value={percent(s.battery_level / 100)} warn={s.battery_level < LOW_BATTERY} />
					{/if}
					{#if s.system?.powered_by}
						<StatusRow icon="plug" label={m.feeder_power_source()} value={POWER[s.system.powered_by]?.() ?? s.system.powered_by} />
					{/if}
					{#if s.is_feeding !== undefined}
						<StatusRow icon="clock" label={m.common_status()} value={s.is_feeding ? m.feeder_feeding() : m.feeder_waiting()} />
					{/if}
					{#if s.system?.fault_status}
						<StatusRow icon="triangle-alert" label={m.common_status()} value={m.feeder_fault()} warn />
					{/if}
					{#if s.error}
						<StatusRow icon="triangle-alert" label={m.common_error()} value={s.error} warn />
					{/if}
				</dl>
			{/if}
		</Section>
	{/snippet}
	{#snippet second()}
		<Section title={m.feeder_schedule()} icon="calendar">
			<p class="hint">{m.feeder_meal_schedule_description()}</p>
			<MealPlanManager {id} />
		</Section>
	{/snippet}
</DeviceTabs>
