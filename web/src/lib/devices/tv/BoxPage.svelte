<script lang="ts">
	// The box's page (/androidtv): wake/sleep, the app shortcuts, a pad, media and volume keys,
	// and under its gear its settings (an admin's; open while it is not configured): its
	// address, the Remote v2 pairing (BoxPairing) and APK sideloading (ApkInstall). Keys go
	// over Remote v2 once paired (~8 ms), over ADB until then (~150 ms). Out of reach, the keys
	// stay, not operable, the tile's state line saying why. The dashboard tile keeps wake/sleep
	// and the app chips only.
	import { m } from '#lib/paraglide/messages.js';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import { androidTvApi, type AndroidKey } from './api.ts';
	import ApkInstall from './ApkInstall.svelte';
	import Apps from './Apps.svelte';
	import BoxPairing from './BoxPairing.svelte';
	import Key from './Key.svelte';
	import Pad from './Pad.svelte';
	import Settings from './Settings.svelte';
	import Volume from './Volume.svelte';
	import { Box } from './box.svelte.ts';
	import { keySender, pacedKeys } from './remote.ts';

	const b = new Box();
	const box = b.box;
	const status = $derived(b.status);
	let settingsOpen = $state(false);
	// the keys show nothing in flight; each its own key, they never wait for one another
	const send = keySender(new Gesture(), (k: AndroidKey) => androidTvApi.sendKey(k));
	// every pad key is an Android key of the same name
	const keys = pacedKeys(send);
	const key = (k: AndroidKey) => () => send(k);
	const id = $props.id();
	/** Out of reach: what the keys are described by (the tile's state line). */
	const why = $derived(b.reachable ? undefined : `${id}-state`);

	const reread = () => box.refresh();
	const closeSettings = async () => {
		await reread();
		settingsOpen = false;
	};
</script>

{#snippet settings()}
	<Settings
		fields={[{ key: 'host', label: m.android_tv_host(), placeholder: '192.168.1.153' }]}
		initial={{ host: b.config?.host }}
		hint={b.configured ? undefined : m.android_tv_configure_hint()}
		save={async (v) => {
			await androidTvApi.setConfig({ ...b.config, ...v });
			await reread();
		}}
		saved={m.android_tv_saved()}
		onsaved={() => (settingsOpen = false)}
	>
		<p class="hint">{m.android_tv_first_run_hint()}</p>
		{#if status?.paired}
			<p class="hint">{m.android_tv_paired_hint()}</p>
		{:else if b.configured}
			<BoxPairing onpaired={closeSettings} />
		{/if}
		{#if b.configured}<ApkInstall oninstalled={reread} />{/if}
	</Settings>
{/snippet}

<PageHead back title={box.data ? b.name : m.android_tv_title()} />

<div class="measure body">
	<Loaded value={box}>
		{#if status}
			<DeviceTile
				level={2}
				name={b.name}
				icon="play"
				state={b.state}
				stateId="{id}-state"
				on={b.configured ? status.awake : undefined}
				offline={b.configured && !b.reachable}
				command={b.awake}
				ontoggle={b.configured ? b.toggle : undefined}
				fact={b.current?.label}
				settings={session.admin ? settings : undefined}
				bind:settingsOpen={() => settingsOpen || !b.configured, (v) => (settingsOpen = v)}
			>
				{#snippet end()}
					<!-- which channel the keys take: the difference between 8 ms and 150 ms a press -->
					{#if status.paired}<span class="chip accent"><Icon name="zap" size={12} />{m.android_tv_paired()}</span>{/if}
				{/snippet}
				{#if !b.configured}<AdminOnly reason={m.android_tv_configure_admin()} />{/if}
				{#if b.configured}<Apps box={b} />{/if}
			</DeviceTile>

			{#if b.configured}
				<Pad label={m.tv_pad({ name: b.name })} {keys} under={['back', 'home', 'menu']} volume reason={why} />
				<div class="row">
					<Key label={m.key_search()} icon="search" fire={key('search')} reason={why} />
					<Key label={m.key_previous()} icon="skip-back" fire={key('previous')} reason={why} />
					<Key label={m.key_play_pause()} icon="play" fire={key('play_pause')} reason={why} />
					<Key label={m.key_next()} icon="skip-forward" fire={key('next')} reason={why} />
				</div>
				<Volume {keys} reason={why} />
			{/if}
		{/if}
	</Loaded>
</div>

<style>
	.body {
		display: grid;
		gap: var(--s-5);
	}
	.row {
		display: flex;
		justify-content: center;
		gap: var(--s-2);
	}
</style>
