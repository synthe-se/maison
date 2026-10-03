<script lang="ts">
	// The feeder's page: serve now (no dialog: the count is in the button, § 6), what it
	// reports (its power, its last meal), and its scheduled meals.
	import { m } from '#lib/paraglide/messages.js';
	import { clock } from '#lib/i18n.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import Section from '#lib/components/Section.svelte';
	import StatusRow from '#lib/components/StatusRow.svelte';
	import type { Device } from './api.ts';
	import { status } from './data.ts';
	import { feed, served } from './tuya.svelte.ts';
	import { lastMeal } from './alerts.ts';
	import DeviceTabs from './DeviceTabs.svelte';
	import MealPlanManager from './MealPlanManager.svelte';
	import Offline from './Offline.svelte';

	/** What the backend says powers it (backend tuya/parse.rs); « Mode 7 », « Unknown » as said. */
	const POWER: Record<string, () => string> = {
		'AC Power': m.feeder_power_mains,
		Battery: m.feeder_power_battery,
		Unknown: m.common_unknown
	};
	/** Portions in one manual serving. */
	const MAX_PORTIONS = 10;

	/** `device`: what the page knows of it (its connection); without it, taken as connected. */
	let { id, device }: { id: string; device?: Pick<Device, 'id' | 'name' | 'connected'> } = $props();
	const offline = $derived(device ? !device.connected : false);
	const uid = $props.id();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const st = live(status('feeder', id));
	const s = $derived(st.data);
	const meal = $derived(lastMeal(s));

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
			/>
			<!-- a stable button: its words say the wait, the focus stays on it -->
			<button class="btn primary wide" {...offline ? unavailable(`${uid}-offline`) : pending(g.is())} onclick={() => !offline && serve()}>
				<Icon name="utensils" busy={g.is()} />{g.is() ? m.feeder_distributing() : m.feeder_distribute({ count: portions })}
			</button>
			{#if offline && device}<Offline {device} id="{uid}-offline" reason={m.cats_offline_reason()} />{/if}
			{#if served[id]}<p class="hint">{m.feeder_served_at({ time: clock(served[id]) })}</p>{/if}
		</Section>

		<Section title={m.feeder_feeder_status()}>
			<Loaded value={st} empty={!s} emptyText={m.feeder_status_fetch_error()}>
				{#if s}
					<dl class="facts">
						{#if s.system?.poweredBy}
							<StatusRow icon="plug" label={m.feeder_power_source()} value={POWER[s.system.poweredBy]?.() ?? s.system.poweredBy} />
						{/if}
						<StatusRow icon="clock" label={m.feeder_last_meal()} value={meal ?? m.common_unknown()} />
					</dl>
				{/if}
			</Loaded>
		</Section>
	{/snippet}
	{#snippet second()}
		<Section title={m.feeder_schedule()} icon="calendar">
			<p class="hint">{m.feeder_meal_schedule_description()}</p>
			<MealPlanManager {id} />
		</Section>
	{/snippet}
</DeviceTabs>
