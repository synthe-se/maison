<script module lang="ts">
	// Discovery outlives a visit to another page: the live value keeps this closure.
	const discovery = $state({ timedOut: false, force: false });
</script>

<script lang="ts">
	// The Mitsubishi air conditioner, driven in infrared through the Broadlink RM4 Pro. The unit
	// never says its state (docs/ux/tableau-de-bord.md § 2, case 3): two buttons, Allumer and
	// Éteindre, and the last command the house sent. The settings are a form; the last command
	// the server persisted fills it once, on mount (AGENTBRIEF.md § 4).
	import { m } from '#lib/paraglide/messages.js';
	import { broadlinkApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import { when } from '#lib/i18n.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Range from '#lib/components/Range.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import Select from '#lib/components/Select.svelte';
	import {
		CLIMATE_OFF,
		STOP_IN_STEP_MIN,
		TEMP_MAX_C,
		TEMP_MIN_C,
		buildClimateCommand,
		settingsFromBackend,
		type ClimateSettings
	} from './climate/command.ts';
	import { degrees, fanOptions, modeLabel, modeOptions, vaneOptions } from './climate/labels.ts';
	import { formatMinutes } from '#lib/format.ts';

	/** The model the backend encodes for (the living-room unit). */
	const MODEL = 'msz-hj5va';
	/** Ask the network every 4 s until the RM4 Pro answers, for 2 min at most. */
	const DISCOVERY_EVERY_MS = 4_000;
	const DISCOVERY_TIMEOUT_MS = 120_000;
	/** Someone may use the physical remote or the IR remote's climate key meanwhile. */
	const STATE_EVERY_MS = 60_000;
	/** Sleep-timer presets, like the remote's 1 h / 3 h buttons (10-minute ticks). */
	const STOP_AFTER_CHOICES = [30, 60, 120, 180, 300, 480, 720];

	const INITIAL: ClimateSettings & { timer: boolean; stopAfter: number } = {
		mode: 'cool',
		temperature: 20,
		fan: 'auto',
		vane: 'auto',
		econo: false,
		timer: false,
		stopAfter: 180
	};

	discovery.timedOut = false;
	const remoteList = live(
		'climate-remote',
		() => broadlinkApi.discover(undefined, discovery.force),
		(d) => (d?.devices.length || discovery.timedOut ? 0 : DISCOVERY_EVERY_MS)
	);
	const stored = live('climate-state', broadlinkApi.getMitsubishiState, STATE_EVERY_MS);
	const remote = $derived(remoteList.data?.devices[0]);
	const last = $derived(stored.data?.state ?? null);

	let form = $state({ ...INITIAL });
	let open = $state(false);
	/** Keyed by the command travelling to the unit. */
	const sending = new Gesture();
	let searching = $state(false);

	const command = $derived(
		buildClimateCommand({ ...form, wide: 'center', stopInMinutes: form.timer ? form.stopAfter : null })
	);

	// one-shot: re-applying the stored state later would clobber an edit in progress
	let hydrated = false;
	$effect(() => {
		const s = stored.data?.state;
		if (hydrated || !s) return;
		hydrated = true;
		if (!s.settings) return;
		const restored = settingsFromBackend(s.settings, INITIAL);
		form = {
			...restored,
			econo: restored.econo ?? false,
			timer: !!restored.stopInMinutes,
			stopAfter: restored.stopInMinutes ?? INITIAL.stopAfter
		};
	});

	// no remote within 2 min: say so and stop asking (the search button asks again)
	$effect(() => {
		if (remote || discovery.timedOut) return;
		const t = setTimeout(() => (discovery.timedOut = true), DISCOVERY_TIMEOUT_MS);
		return () => clearTimeout(t);
	});

	/** A fresh search of the network (an admin's: it asks the server to scan again), once. */
	async function search() {
		if (searching) return;
		discovery.timedOut = false;
		discovery.force = true;
		searching = true;
		try {
			await remoteList.refresh();
		} finally {
			// the polls after it read the server's cache again
			discovery.force = false;
			searching = false;
		}
	}

	function send(cmd: string) {
		if (!remote) return;
		const { host } = remote;
		return sending.run(
			() => broadlinkApi.sendMitsubishiCommand(host, cmd, MODEL),
			async () => {
				ui.say(m.climate_command_sent());
				await stored.refresh();
			},
			cmd
		);
	}

	const lastLine = $derived(
		!last ? m.climate_no_order() : last.power ? m.climate_last_on({ when: when(last.updatedAt) }) : m.climate_last_off({ when: when(last.updatedAt) })
	);
	const fact = $derived(
		last?.power && last.settings
			? m.climate_fact({ temperature: degrees(last.settings.temperature), mode: modeLabel(last.settings.mode) })
			: undefined
	);

	// a restored timer may carry any multiple of 10 minutes: keep it in the list
	const stopOptions = $derived(
		[...new Set([...STOP_AFTER_CHOICES, form.stopAfter])]
			.filter((n) => Number.isInteger(n) && n > 0 && n % STOP_IN_STEP_MIN === 0)
			.sort((a, b) => a - b)
			.map((n) => ({ value: String(n), label: formatMinutes(n) }))
	);
</script>

<section class="group" aria-labelledby="climate-title">
	<div class="group-head">
		<h2 id="climate-title" class="group-title">{m.climate_dashboard_title()}</h2>
		<AdminOnly reason={false}>
			<button class="icon-btn end" aria-label={m.climate_search_remote()} {...pending(searching)} onclick={search}>
				<Icon name="refresh-cw" busy={searching} />
			</button>
		</AdminOnly>
	</div>

	<div class="tiles">
		<DeviceTile name={m.climate_brand_name()} icon="snowflake" state={lastLine} {fact} warn={!remote && discovery.timedOut}>
			{#snippet end()}
				<button class="icon-btn" aria-label={m.climate_settings()} aria-expanded={open} onclick={() => (open = !open)}>
					<Icon name="settings-2" />
				</button>
			{/snippet}

			{#if !remote}
				{#if discovery.timedOut}
					<div class="hint">
						<p class="warn-text">{m.climate_no_remote_title()}</p>
						<p>{m.climate_no_remote_description()}</p>
					</div>
				{:else}
					<p class="hint searching"><Icon name="search" busy />{m.climate_searching_title()} — {m.climate_searching_description()}</p>
				{/if}
			{/if}

			<div class="btn-row">
				<button class="btn" disabled={!remote} {...pending(sending.is())} onclick={() => !sending.is() && send(command)}>
					<Icon name="power" busy={sending.is(command)} /><span class="btn-text">{m.action_turn_on()}</span>
				</button>
				<button class="btn" disabled={!remote} {...pending(sending.is())} onclick={() => !sending.is() && send(CLIMATE_OFF)}>
					<Icon name="square" busy={sending.is(CLIMATE_OFF)} /><span class="btn-text">{m.action_turn_off()}</span>
				</button>
			</div>

			{#if open}
				<div class="inset settings">
					{#if remote}<p class="hint">{m.climate_remote_connected({ host: remote.host })}</p>{/if}
					<Select
						label={m.climate_mode()}
						value={form.mode}
						options={modeOptions()}
						onchange={(v) => (form = { ...form, mode: v, econo: v === 'cool' ? form.econo : false })}
					/>
					<Range
						label={m.climate_temperature()}
						value={form.temperature}
						min={TEMP_MIN_C}
						max={TEMP_MAX_C}
						valueText={(v) => m.climate_degrees_words({ degrees: v })}
						oncommit={(v) => (form.temperature = v)}
					/>
					<Select label={m.climate_fan()} value={form.fan} options={fanOptions()} onchange={(v) => (form.fan = v)} />
					<Select label={m.climate_vertical_vane()} value={form.vane} options={vaneOptions()} onchange={(v) => (form.vane = v)} />

					<div class="field">
						<span class="label" id="climate-timer-label">{m.climate_timer_mode()}</span>
						<div class="btn-row segmented" role="group" aria-labelledby="climate-timer-label">
							<button class="btn" aria-pressed={!form.timer} onclick={() => (form.timer = false)}>{m.climate_timer_modes_none()}</button>
							<button class="btn" aria-pressed={form.timer} onclick={() => (form.timer = true)}>{m.climate_timer_modes_stop()}</button>
						</div>
					</div>
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
							checked={form.econo ?? false}
							disabled={form.mode !== 'cool'}
							onchange={(v) => (form.econo = v)}
						/>
						<p class="hint">{m.climate_econo_cool_description()}</p>
					</div>

					<!-- the infrared order itself: for whoever debugs, folded -->
					<details class="raw">
						<summary class="link-btn quiet">{m.climate_generated_command()}</summary>
						<code>{command}</code>
					</details>

					<div class="actions">
						<button class="btn primary" disabled={!remote} {...pending(sending.is())} onclick={() => !sending.is() && send(command)}>
							<Icon name="power" busy={sending.is(command)} />{m.climate_send_structured_command()}
						</button>
						<button class="btn" onclick={() => (form = { ...INITIAL })}>{m.climate_reset()}</button>
					</div>
				</div>
			{/if}
		</DeviceTile>
	</div>
</section>

<style>
	.end { margin-left: auto; }
	.settings { gap: var(--s-4); }
	.searching { display: flex; align-items: center; gap: var(--s-2); }
	.hint p { margin: 0; }
	.raw summary { cursor: pointer; font: var(--t-secondary); min-height: var(--control-h-xs); display: flex; align-items: center; }
	code { overflow-wrap: anywhere; color: var(--ink); font: var(--t-secondary); }
	/* « Éteindre après un délai » wraps rather than overflow at 390 px */
	.segmented .btn { white-space: normal; text-align: center; }
</style>
