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
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
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

	/** Not the group's name (« Android TV » > « Box du salon »). */
	const name = $derived(status?.model || m.android_tv_box());
	// waking the box wakes the TV through CEC: the TV's 30 s limit
	const awake = new Command(() => name, LIMIT.tv);
	let settingsOpen = $state(false);
	let settingsButton = $state<HTMLButtonElement>();
	// independent gestures, each free to travel while another does without clearing its mark:
	// launching an app (keyed by package; any launch holds the shortcuts), the pairing, and the
	// keys and the APK upload, which show nothing in flight (the upload has its own progress)
	const apps = new Gesture();
	const pairing = new Gesture();
	const pad = new Gesture();
	// each key its own gesture: keys never wait for one another (the pacing is remote.ts's)
	let sent = 0;
	const run = (send: () => Promise<unknown>) => pad.run(send, undefined, `key-${++sent}`);

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
	/** The code typed is not six characters: said under the field. */
	let codeProblem = $state('');

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

	/** The code the TV shows: six characters. */
	const CODE_LENGTH = 6;

	function pairFinish(e: SubmitEvent) {
		e.preventDefault();
		const code = pairCode.trim();
		codeProblem = code.length === CODE_LENGTH ? '' : m.android_tv_pair_code_length({ count: CODE_LENGTH });
		if (codeProblem) return void codeInput?.focus();
		return pairing.run(
			() => androidTvApi.pairFinish(code),
			async () => {
				haptic(CONFIRM);
				ui.toast(m.android_tv_paired_ok());
				pairOpen = false;
				pairCode = '';
				await box.refresh();
				// the pairing form is gone: the focus goes back to the settings' button
				await refocus(settingsButton);
			},
			'finish',
			{ field: () => codeInput }
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
			},
			'apk'
		);
		apk = null;
		// the upload's progress gave way to the button again: the focus goes back to it
		await refocus('#atv-apk');
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

	<Loaded value={box}>
		<div class="tiles">
			{#if status}
				<DeviceTile
					{name}
					icon="play"
					state={describe}
					on={configured ? status.awake : undefined}
					offline={configured && !reachable}
					command={awake}
					ontoggle={configured ? toggle : undefined}
					fact={current?.label}
				>
					{#snippet end()}
						<!-- which channel the keys take: the difference between 8 ms and 150 ms a press -->
						{#if status.paired}<span class="chip accent"><Icon name="zap" size={12} />{m.android_tv_paired()}</span>{/if}
						<AdminOnly reason={false}>
							<button
								class="icon-btn"
								bind:this={settingsButton}
								aria-label={m.common_settings_of({ name })}
								aria-expanded={settingsOpen}
								onclick={() => (settingsOpen = !settingsOpen)}><Icon name="settings-2" /></button
							>
						</AdminOnly>
					{/snippet}

					{#if settingsOpen || !configured}
						<AdminOnly reason={configured ? false : m.android_tv_configure_admin()}>
						<Settings
							fields={[{ key: 'host', label: m.android_tv_host(), placeholder: '192.168.1.153' }]}
							initial={{ host: config?.host }}
							hint={configured ? undefined : m.android_tv_configure_hint()}
							save={async (v) => {
								await androidTvApi.setConfig({ ...config, ...v });
								await box.refresh();
							}}
							saved={m.android_tv_saved()}
							onsaved={() => {
								settingsOpen = false;
								void refocus(settingsButton);
							}}
						>
							<p class="hint">{m.android_tv_first_run_hint()}</p>
							{#if status.paired}
								<p class="hint">{m.android_tv_paired_hint()}</p>
							{:else if configured}
								<!-- Remote v2 pairing: optional, but it is what makes keys fast -->
								<div class="block">
									<p class="hint">{m.android_tv_pair_hint()}</p>
									{#if pairOpen}
										<form class="pair" onsubmit={pairFinish} novalidate>
											<div class="actions">
												<div class="field">
													<label for="atv-pair">{m.android_tv_pair_code()}</label>
													<input
														id="atv-pair"
														bind:this={codeInput}
														bind:value={pairCode}
														oninput={() => (codeProblem = '')}
														maxlength={CODE_LENGTH}
														autocomplete="one-time-code"
														autocapitalize="characters"
														spellcheck="false"
														aria-invalid={codeProblem || pairing.error ? 'true' : undefined}
														aria-describedby="atv-pair-error"
													/>
												</div>
												<button class="btn primary" {...pending(pairing.is('finish'))}>
													<Icon name="check" busy={pairing.is('finish')} />{m.android_tv_pair_confirm()}
												</button>
											</div>
											<p class="form-error" id="atv-pair-error">{codeProblem || pairing.error}</p>
										</form>
									{:else}
										<div class="actions">
											<!-- a stable button: its label says the wait, the focus stays on it -->
											<button class="btn" {...pending(pairing.is())} onclick={pairStart}>
												<Icon name="zap" busy={pairing.is()} />{pairing.is() ? m.android_tv_pairing() : m.android_tv_pair()}
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
									<p class="hint"><Icon name="upload" busy />{m.android_tv_installing()}</p>
								{:else}
									<div class="actions">
										<label class="btn file">
											<Icon name="upload" />{m.android_tv_install_apk()}
											<input id="atv-apk" class="sr-only" type="file" accept=".apk,application/vnd.android.package-archive" onchange={install} />
										</label>
									</div>
								{/if}
							{/if}
						</Settings>
						</AdminOnly>
					{/if}

					{#if configured}
						<div class="apps" role="group" aria-label={m.android_tv_apps()}>
							{#each shortcuts as app (app.package)}
								<button
									class="pill-btn"
									class:on={status.currentApp === app.package}
									aria-current={status.currentApp === app.package ? 'true' : undefined}
									disabled={!reachable}
									{...pending(apps.is())}
									onclick={() => !apps.is() && launch(app)}
								>
									{#if apps.is(app.package)}<Icon name="play" busy />{/if}{app.label}
								</button>
							{/each}
						</div>

						<More>
							<Pad label={m.tv_pad({ name })} {keys} under={['back', 'home', 'menu']} volume disabled={!reachable} />
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
	</Loaded>
</section>

<style>
	.apps { display: flex; flex-wrap: wrap; gap: var(--s-2); }
	.apps .pill-btn { min-height: var(--control-h); }
	.row { display: flex; justify-content: center; gap: var(--s-2); }
	.pair .actions { align-items: end; }
	.pair input { width: 9ch; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; text-transform: uppercase; letter-spacing: 0.2em; }
	.file { position: relative; }
	.file:focus-within { outline: 3px solid var(--accent); outline-offset: 2px; }
	.hint :global(svg) { vertical-align: middle; margin-right: var(--s-1); }
</style>
