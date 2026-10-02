<script lang="ts">
	// The litter box's page: clean now, the delay before it cleans itself, its levels; then its
	// night hours and preferences as switches (§ 2, case 2).
	import { m } from '#lib/paraglide/messages.js';
	import { litterBoxApi } from '#lib/api.ts';
	import { formatDuration } from '#lib/format.ts';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import DeviceTabs from './DeviceTabs.svelte';
	import Section from './Section.svelte';
	import SettingRow from '#lib/components/SettingRow.svelte';
	import StatusRow from './StatusRow.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { reread, status } from './tuya.svelte.ts';

	type Settings = Parameters<typeof litterBoxApi.settings>[1];
	type Preference = keyof NonNullable<Settings['preferences']>;
	interface LitterStatus {
		clean_delay?: { seconds?: number };
		sleep_mode?: { enabled?: boolean; start_time_formatted?: string; end_time_formatted?: string };
		sensors?: { litter_level?: string; fault_alarm?: number };
		system?: { state?: string; maintenance_required?: boolean };
		settings?: Partial<Record<Preference, boolean>>;
	}

	const STATE: Record<string, () => string> = {
		cleaning: m.litter_box_status_cleaning,
		cat_inside: m.litter_box_status_cat_inside,
		clumping: m.litter_box_status_clumping,
		satnd_by: m.litter_box_status_standby // sic: the device's spelling
	};
	const LEVEL: Record<string, () => string> = { full: m.litter_box_filled, half: m.litter_box_half_filled };
	const PREFERENCES: { key: Preference; icon: IconName; label: () => string }[] = [
		{ key: 'child_lock', icon: 'lock', label: m.litter_box_child_lock },
		{ key: 'kitten_mode', icon: 'baby', label: m.litter_box_kitten_mode },
		{ key: 'lighting', icon: 'lightbulb', label: m.litter_box_lighting },
		{ key: 'prompt_sound', icon: 'volume-2', label: m.litter_box_sounds },
		{ key: 'automatic_homing', icon: 'house', label: m.litter_box_automatic_homing }
	];
	/** The device's bounds for the delay before it cleans itself, in seconds. */
	const DELAY = { min: 60, max: 1800, step: 60, fallback: 120 };
	const NIGHT = { start: '23:00', end: '07:00' };

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const st = status<LitterStatus>('litter-box', id);
	const s = $derived(st.data);
	const g = new Gesture();
	const prefix = $derived(`tuya:${id}`);
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
	const start = $derived(startDraft ?? s?.sleep_mode?.start_time_formatted ?? NIGHT.start);
	const end = $derived(endDraft ?? s?.sleep_mode?.end_time_formatted ?? NIGHT.end);

	async function applyNight(e: SubmitEvent) {
		e.preventDefault();
		await save('night', { sleep_mode: { start_time: start, end_time: end } }, () => (startDraft = endDraft = null));
	}
	async function clean() {
		await g.run(() => litterBoxApi.clean(id), reread(prefix, m.litter_box_cleaning_started()), 'clean');
	}
</script>

{#if st.loading}
	<p class="hint" role="status">{m.common_loading()}</p>
{:else}
	<DeviceTabs other={{ label: m.litter_box_settings(), icon: 'settings' }}>
		{#snippet controls()}
			<Section title={m.litter_box_litter_status()} icon="trash">
				<button class="btn primary wide" disabled={g.is('clean')} aria-busy={g.is('clean')} onclick={clean}>
					{#if g.is('clean')}<Icon name="loader-circle" class="spin" />{m.litter_box_cleaning()}{:else}<Icon name="trash" />{m.litter_box_start_cleaning()}{/if}
				</button>
				<Range
					label={m.litter_box_clean_delay_before()}
					value={s?.clean_delay?.seconds || DELAY.fallback}
					min={DELAY.min}
					max={DELAY.max}
					step={DELAY.step}
					valueText={formatDuration}
					disabled={g.is('delay')}
					oncommit={(v) => save('delay', { clean_delay: v })}
				/>
				<dl class="facts">
					{#if s?.sensors?.litter_level}
						<StatusRow
							icon="gauge"
							label={m.litter_box_litter_level()}
							value={LEVEL[s.sensors.litter_level]?.() ?? s.sensors.litter_level}
							warn={s.sensors.litter_level === 'half'}
						/>
					{/if}
					<StatusRow icon="clock" label={m.common_status()} value={(s?.system?.state && STATE[s.system.state]?.()) || s?.system?.state || m.common_unknown()} />
					{#if s?.system?.maintenance_required}
						<StatusRow icon="settings" label={m.common_status()} value={m.litter_box_maintenance_required()} warn />
					{/if}
					{#if s?.sensors?.fault_alarm}
						<StatusRow icon="triangle-alert" label={m.common_error()} value={m.litter_box_fault_alarm({ code: s.sensors.fault_alarm })} warn />
					{/if}
				</dl>
				{#if s?.sensors?.litter_level === 'half'}<p class="hint">{m.litter_box_fill_soon()}</p>{/if}
				<ConfirmDialog
					label={m.litter_box_reset_litter_level()}
					icon="refresh-cw"
					title={m.litter_box_reset_litter_level_title()}
					description={m.litter_box_reset_litter_level_description()}
					action={m.litter_box_reset_action()}
					pending={g.is('level')}
					onconfirm={() => save('level', { actions: { reset_sand_level: true } })}
				/>
			</Section>
		{/snippet}
		{#snippet second()}
			<Section title={m.litter_box_night_mode()} icon="moon">
				<ul class="settings">
					<SettingRow
						icon="moon"
						label={m.litter_box_night_mode()}
						hint={m.litter_box_night_mode_description()}
						checked={s?.sleep_mode?.enabled ?? false}
						pending={g.is('night-on')}
						onchange={(enabled) => save('night-on', { sleep_mode: { enabled } })}
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
					<button class="btn wide" disabled={g.is('night')} aria-busy={g.is('night')}>
						{#if g.is('night')}<Icon name="loader-circle" class="spin" />{/if}{m.litter_box_apply_schedule()}
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
							pending={g.is(p.key)}
							onchange={(on) => save(p.key, { preferences: { [p.key]: on } })}
						/>
					{/each}
				</ul>
			</Section>
		{/snippet}
	</DeviceTabs>
{/if}

<style>
	.night { display: grid; gap: var(--s-3); }
	fieldset { border: 0; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--s-3); }
	legend { font-weight: 600; font-size: 0.85rem; padding: 0; margin-bottom: var(--s-2); }
</style>
