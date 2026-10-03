<script lang="ts">
	// The litter box's page: clean now, the delay before it cleans itself, its levels; then its
	// night hours and preferences as switches (§ 2, case 2). Offline, « Lancer le nettoyage »
	// stays in place but unavailable, said why with a « Reconnecter » (§ 2); the delay and
	// « J'ai rempli » wait for the connection (nothing that looks operable and is not).
	import { m } from '#lib/paraglide/messages.js';
	import { formatDuration } from '#lib/format.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Section from '#lib/components/Section.svelte';
	import SettingRow from '#lib/components/SettingRow.svelte';
	import StatusRow from '#lib/components/StatusRow.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { litterBoxApi, type Device, type LitterBoxPreference as Preference, type LitterBoxSettings as Settings } from './api.ts';
	import { status, TUYA } from './data.ts';
	import { reread } from './tuya.svelte.ts';
	import DeviceTabs from './DeviceTabs.svelte';
	import Offline from './Offline.svelte';

	const STATE: Record<string, () => string> = {
		cleaning: m.litter_box_status_cleaning,
		cat_inside: m.litter_box_status_cat_inside,
		clumping: m.litter_box_status_clumping,
		satnd_by: m.state_standby // sic: the device's spelling
	};
	const LEVEL: Record<string, () => string> = { full: m.litter_box_filled, half: m.litter_box_half_filled };
	const PREFERENCES: { key: Preference; icon: IconName; label: () => string }[] = [
		{ key: 'childLock', icon: 'lock', label: m.litter_box_child_lock },
		{ key: 'kittenMode', icon: 'baby', label: m.litter_box_kitten_mode },
		{ key: 'lighting', icon: 'lightbulb', label: m.litter_box_lighting },
		{ key: 'promptSound', icon: 'volume-2', label: m.litter_box_sounds },
		{ key: 'automaticHoming', icon: 'house', label: m.litter_box_automatic_homing }
	];
	/** The device's bounds for the delay before it cleans itself, in seconds. */
	const DELAY = { min: 60, max: 1800, step: 60, fallback: 120 };
	const NIGHT = { start: '23:00', end: '07:00' };

	/** `device`: what the page knows of it (its connection); without it, taken as connected. */
	let { id, device }: { id: string; device?: Pick<Device, 'id' | 'name' | 'connected'> } = $props();
	const offline = $derived(device ? !device.connected : false);
	const uid = $props.id();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const st = live(status('litter-box', id));
	const s = $derived(st.data);
	const g = new Gesture();
	const prefix = $derived(`${TUYA}${id}`);
	const save = (key: string, settings: Settings, then?: () => void) =>
		g.run(
			() => litterBoxApi.settings(id, settings),
			async () => {
				await reread(prefix, m.device_setting_saved())();
				then?.();
			},
			key
		);

	// the night hours typed but not yet applied; the device's value otherwise
	let startDraft = $state<string | null>(null);
	let endDraft = $state<string | null>(null);
	const start = $derived(startDraft ?? s?.sleepMode?.startTimeFormatted ?? NIGHT.start);
	const end = $derived(endDraft ?? s?.sleepMode?.endTimeFormatted ?? NIGHT.end);

	async function applyNight(e: SubmitEvent) {
		e.preventDefault();
		await save('night', { sleepMode: { startTime: start, endTime: end } }, () => (startDraft = endDraft = null));
	}
	async function clean() {
		await g.run(() => litterBoxApi.clean(id), reread(prefix, m.litter_box_cleaning_started()), 'clean');
	}
</script>

<Loaded value={st}>
	<DeviceTabs other={{ label: m.common_settings(), icon: 'settings' }}>
		{#snippet controls()}
			<Section title={m.litter_box_litter_status()} icon="trash">
				<button
					class="btn primary wide"
					{...offline ? unavailable(`${uid}-offline`) : pending(g.is('clean'))}
					onclick={() => !offline && clean()}
				>
					<Icon name="trash" busy={g.is('clean')} />{g.is('clean') ? m.litter_box_cleaning() : m.litter_box_start_cleaning()}
				</button>
				{#if offline && device}
					<Offline {device} id="{uid}-offline" reason={m.litter_box_offline_reason()} />
				{:else}
					<Range
						label={m.litter_box_clean_delay_before()}
						value={s?.cleanDelay?.seconds || DELAY.fallback}
						min={DELAY.min}
						max={DELAY.max}
						step={DELAY.step}
						valueText={formatDuration}
						oncommit={(v) => save('delay', { cleanDelay: v })}
					/>
				{/if}
				<dl class="facts">
					{#if s?.sensors?.litterLevel}
						<StatusRow
							icon="gauge"
							label={m.litter_box_litter_level()}
							value={LEVEL[s.sensors.litterLevel]?.() ?? s.sensors.litterLevel}
							warn={s.sensors.litterLevel === 'half'}
						/>
					{/if}
					<StatusRow
						icon="clock"
						label={m.common_status()}
						value={(s?.system?.state && STATE[s.system.state]?.()) || s?.system?.state || m.common_unknown()}
					/>
					{#if s?.system?.maintenanceRequired}
						<StatusRow icon="settings" label={m.common_status()} value={m.litter_box_maintenance_required()} warn />
					{/if}
					{#if s?.sensors?.faultAlarm}
						<StatusRow
							icon="triangle-alert"
							label={m.common_error()}
							value={m.litter_box_fault_alarm({ code: s.sensors.faultAlarm })}
							warn
						/>
					{/if}
				</dl>
				{#if s?.sensors?.litterLevel === 'half'}<p class="hint">{m.litter_box_fill_soon()}</p>{/if}
				{#if !offline}
					<ConfirmDialog
						label={m.litter_box_reset_litter_level()}
						icon="refresh-cw"
						title={m.litter_box_reset_litter_level_title()}
						description={m.litter_box_reset_litter_level_description()}
						action={m.litter_box_reset_action()}
						busy={g.is('level')}
						onconfirm={() => save('level', { actions: { resetSandLevel: true } })}
					/>
				{/if}
			</Section>
		{/snippet}
		{#snippet second()}
			<Section title={m.litter_box_night_mode()} icon="moon">
				<ul class="settings">
					<SettingRow
						icon="moon"
						label={m.litter_box_night_mode()}
						hint={m.litter_box_night_mode_description()}
						checked={s?.sleepMode?.enabled ?? false}
						busy={g.is('night-on')}
						onchange={(enabled) => save('night-on', { sleepMode: { enabled } })}
					/>
				</ul>
				<form class="night" onsubmit={applyNight}>
					<fieldset>
						<legend>{m.litter_box_night_hours()}</legend>
						<div class="field">
							<label for="night-start-{id}">{m.litter_box_start()}</label>
							<input id="night-start-{id}" type="time" value={start} oninput={(e) => (startDraft = e.currentTarget.value)} required />
						</div>
						<div class="field">
							<label for="night-end-{id}">{m.litter_box_end()}</label>
							<input id="night-end-{id}" type="time" value={end} oninput={(e) => (endDraft = e.currentTarget.value)} required />
						</div>
					</fieldset>
					<button class="btn wide" {...pending(g.is('night'))}>
						<Icon name="clock" busy={g.is('night')} />{m.litter_box_apply_schedule()}
					</button>
				</form>
			</Section>
			<Section title={m.litter_box_preferences()} icon="settings">
				<ul class="settings">
					{#each PREFERENCES as p (p.key)}
						<SettingRow
							icon={p.icon}
							label={p.label()}
							checked={s?.settings?.[p.key] ?? false}
							busy={g.is(p.key)}
							onchange={(on) => save(p.key, { preferences: { [p.key]: on } })}
						/>
					{/each}
				</ul>
			</Section>
		{/snippet}
	</DeviceTabs>
</Loaded>

<style>
	.night {
		display: grid;
		gap: var(--s-3);
	}
	fieldset {
		border: 0;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--s-3);
	}
	legend {
		font: var(--t-field-label);
		padding: 0;
		margin-bottom: var(--s-2);
	}
</style>
