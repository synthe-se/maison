<script lang="ts">
	// The Android TV box (MECOOL LEAP-S1): wake/sleep (CEC follows: waking it turns the TV on
	// and takes its input, sleeping it puts the TV to standby), a pad, media and volume keys,
	// app shortcuts, and in its settings the Remote v2 pairing and APK sideloading. Keys go over
	// Remote v2 once paired (~8 ms), over ADB until then (~150 ms).
	import { tick } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { androidTvApi, type AndroidApp, type AndroidKey } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Command, LIMIT } from '#lib/command.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Key from './tv/Key.svelte';
	import Pad from './tv/Pad.svelte';
	import More from './tv/More.svelte';
	import Progress from '#lib/components/Progress.svelte';
	import Settings from './tv/Settings.svelte';
	import { pacedKeys, REMOTE_POLL } from './tv/remote.ts';

	/** Apps worth a shortcut, keyed by the packages actually on the box (config can override). */
	const SHORTCUTS: AndroidApp[] = [
		{ package: 'org.smarttube.beta', label: 'SmartTube' },
		{ package: 'studio.kahn.iris.tv', label: 'Iris' }
	];

	// polled as slowly as the TV (see REMOTE_POLL): waking the box drives the TV over CEC
	const box = live('androidtv', androidTvApi.status, REMOTE_POLL);
	const status = $derived(box.data?.status);
	const config = $derived(box.data?.config);
	const configured = $derived(status?.configured ?? false);
	const reachable = $derived(status?.reachable ?? false);
	const shortcuts = $derived(config?.favouriteApps?.length ? config.favouriteApps : SHORTCUTS);
	const current = $derived(shortcuts.find((a) => a.package === status?.currentApp));

	// waking the box wakes the TV through CEC: the TV's 30 s limit
	const awake = new Command(() => m.android_tv_title(), LIMIT.tv);
	let settingsOpen = $state(false);
	// independent gestures, each free to travel while another does without clearing its mark:
	// launching an app (keyed by package; any launch holds the shortcuts), the pairing, and the
	// keys and the APK upload, which show nothing in flight (the upload has its own progress)
	const apps = new Gesture();
	const pairing = new Gesture();
	const pad = new Gesture();
	const run = (send: () => Promise<unknown>) => pad.run(send);

	async function toggle(on: boolean) {
		haptic(CONFIRM);
		const ok = await awake.run(on, () => (on ? androidTvApi.wake() : androidTvApi.sleep()));
		if (ok && box.data) box.set({ ...box.data, status: { ...box.data.status, awake: on } });
		if (ok) void box.refresh();
	}

	// every pad key is an Android key of the same name
	const keys = pacedKeys((k) => run(() => androidTvApi.sendKey(k)));
	const key = (k: AndroidKey) => () => run(() => androidTvApi.sendKey(k));

	function launch(app: AndroidApp) {
		haptic(CONFIRM);
		return apps.run(
			() => androidTvApi.launch(app.package),
			() => {
				ui.toast(m.android_tv_launched({ name: app.label }));
				void box.refresh();
			},
			app.package
		);
	}

	// ── Remote v2 pairing: two calls with a person reading a code off the TV in between ──
	let pairOpen = $state(false);
	let pairCode = $state('');
	let codeInput = $state<HTMLInputElement>();

	function pairStart() {
		return pairing.run(
			() => androidTvApi.pairStart(),
			async () => {
				pairOpen = true;
				ui.say(m.android_tv_pair_started());
				await tick();
				codeInput?.focus();
			}
		);
	}

	function pairFinish(e: SubmitEvent) {
		e.preventDefault();
		return pairing.run(
			() => androidTvApi.pairFinish(pairCode.trim()),
			async () => {
				haptic(CONFIRM);
				ui.toast(m.android_tv_paired_ok());
				pairOpen = false;
				pairCode = '';
				await box.refresh();
			}
		);
	}

	// ── APK sideloading: the upload in percent, then « Installation… » (§ 6) ──
	let apk = $state<{ name: string; sent: number } | null>(null);

	async function install(e: Event & { currentTarget: HTMLInputElement }) {
		const file = e.currentTarget.files?.[0];
		e.currentTarget.value = '';
		if (!file) return;
		apk = { name: file.name, sent: 0 };
		await pad.run(
			() => androidTvApi.installApk(file, (sent) => apk && (apk.sent = sent)),
			() => {
				haptic(CONFIRM);
				ui.toast(m.android_tv_apk_installed({ name: file.name }));
				void box.refresh();
			}
		);
		apk = null;
	}

	const describe = $derived(
		!configured
			? m.android_tv_not_configured()
			: !reachable
				? m.state_unreachable()
				: status?.awake
					? m.android_tv_awake()
					: m.android_tv_asleep()
	);
</script>

<section class="group" aria-labelledby="android-tv-title">
	<div class="group-head">
		<h2 id="android-tv-title" class="group-title">{m.android_tv_title()}</h2>
	</div>

	{#if box.loading}
		<p class="hint" role="status">{m.common_loading()}</p>
	{:else}
		<div class="tiles">
			{#if !status}
				<DeviceTile name={m.android_tv_title()} icon="play" state={m.command_no_answer_short()} warn />
			{:else}
				<DeviceTile
					name={m.android_tv_title()}
					icon="play"
					state={describe}
					on={reachable ? status.awake : undefined}
					command={awake}
					ontoggle={reachable ? toggle : undefined}
					warn={configured && !reachable}
					fact={current?.label}
				>
					{#snippet end()}
						<!-- which channel the keys take: the difference between 8 ms and 150 ms a press -->
						{#if status.paired}<span class="chip accent"><Icon name="zap" size={12} />{m.android_tv_paired()}</span>{/if}
						<button
							class="icon-btn"
							aria-label={m.common_settings()}
							aria-expanded={settingsOpen}
							onclick={() => (settingsOpen = !settingsOpen)}><Icon name="settings-2" /></button
						>
					{/snippet}

					{#if settingsOpen || !configured}
						<Settings
							fields={[{ key: 'host', label: m.android_tv_host(), placeholder: '192.168.1.153' }]}
							initial={{ host: config?.host }}
							hint={configured ? undefined : m.android_tv_configure_hint()}
							save={async (v) => {
								await androidTvApi.setConfig({ ...config, ...v });
								await box.refresh();
							}}
							saved={m.android_tv_saved()}
							onsaved={() => (settingsOpen = false)}
						>
							<p class="hint">{m.android_tv_first_run_hint()}</p>
							{#if status.paired}
								<p class="hint">{m.android_tv_paired_hint()}</p>
							{:else if configured}
								<!-- Remote v2 pairing: optional, but it is what makes keys fast -->
								<div class="block">
									<p class="hint">{m.android_tv_pair_hint()}</p>
									{#if pairOpen}
										<form class="actions pair" onsubmit={pairFinish}>
											<div class="field">
												<label for="atv-pair">{m.android_tv_pair_code()}</label>
												<input
													id="atv-pair"
													bind:this={codeInput}
													bind:value={pairCode}
													maxlength={6}
													autocomplete="one-time-code"
													autocapitalize="characters"
													spellcheck="false"
												/>
											</div>
											<button class="btn primary" disabled={pairCode.trim().length !== 6 || pairing.is()}>
												{#if pairing.is()}<Icon name="loader-circle" class="spin" />{/if}{m.android_tv_pair_confirm()}
											</button>
										</form>
									{:else}
										<div class="actions">
											<button class="btn" disabled={pairing.is()} onclick={pairStart}>
												{#if pairing.is()}<Icon name="loader-circle" class="spin" />{m.android_tv_pairing()}{:else}<Icon
														name="zap"
													/>{m.android_tv_pair()}{/if}
											</button>
										</div>
									{/if}
								</div>
							{/if}

							{#if configured}
								<!-- sideloading: the box is plain Android, so an APK just installs -->
								{#if apk && apk.sent < 1}
									<Progress
										label={m.android_tv_uploading({ name: apk.name })}
										value={apk.sent * 100}
										max={100}
										valueText={m.android_tv_upload_sent({ percent: Math.round(apk.sent * 100) })}
									/>
								{:else if apk}
									<p class="hint"><Icon name="loader-circle" class="spin" />{m.android_tv_installing()}</p>
								{:else}
									<div class="actions">
										<label class="btn file">
											<Icon name="upload" />{m.android_tv_install_apk()}
											<input class="sr-only" type="file" accept=".apk,application/vnd.android.package-archive" onchange={install} />
										</label>
									</div>
								{/if}
							{/if}
						</Settings>
					{/if}

					{#if configured}
						<div class="apps" role="group" aria-label={m.android_tv_apps()}>
							{#each shortcuts as app (app.package)}
								<button
									class="pill-btn"
									class:on={status.currentApp === app.package}
									aria-current={status.currentApp === app.package ? 'true' : undefined}
									disabled={!reachable || apps.is()}
									onclick={() => launch(app)}
								>
									{#if apps.is(app.package)}<Icon name="loader-circle" class="spin" />{/if}{app.label}
								</button>
							{/each}
						</div>

						<More>
							<Pad label={m.tv_pad({ name: m.android_tv_title() })} {keys} under={['back', 'home', 'menu']} volume disabled={!reachable} />
							<div class="row">
								<Key label={m.remote_keys_search()} icon="search" fire={key('search')} disabled={!reachable} />
								<Key label={m.tv_key_previous()} icon="skip-back" fire={key('previous')} disabled={!reachable} />
								<Key label={m.tv_key_play_pause()} icon="play" fire={key('play_pause')} disabled={!reachable} />
								<Key label={m.tv_key_next()} icon="skip-forward" fire={key('next')} disabled={!reachable} />
							</div>
							<div class="row">
								<Key k="volume_down" fire={keys.volume_down} disabled={!reachable} />
								<Key k="mute" fire={keys.mute} disabled={!reachable} />
								<Key k="volume_up" fire={keys.volume_up} disabled={!reachable} />
							</div>
						</More>
					{/if}
				</DeviceTile>
			{/if}
		</div>
	{/if}
</section>

<style>
	.apps { display: flex; flex-wrap: wrap; gap: var(--s-2); }
	.apps .pill-btn { min-height: var(--control-h); }
	.row { display: flex; justify-content: center; gap: var(--s-2); }
	.block { display: grid; gap: var(--s-2); }
	.pair { align-items: end; }
	.pair input { width: 9ch; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; text-transform: uppercase; letter-spacing: 0.2em; }
	.file { position: relative; }
	.file:focus-within { outline: 3px solid var(--accent); outline-offset: 2px; }
	.hint :global(svg) { vertical-align: middle; margin-right: var(--s-1); }
</style>
