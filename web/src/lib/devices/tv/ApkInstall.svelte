<script lang="ts">
	// Sideloading an APK onto the box (an admin's, in its settings): the box is plain Android,
	// so an APK just installs. Long steps named and timed (§ 6): the upload in percent, then
	// « Installation… », then said installed; the focus goes back to the button after.
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Progress from '#lib/components/Progress.svelte';
	import { androidTvApi } from './api.ts';

	let { oninstalled }: { oninstalled: () => unknown } = $props();
	const id = $props.id();
	const g = new Gesture();
	let apk = $state<{ name: string; sent: number } | null>(null);

	async function install(e: Event & { currentTarget: HTMLInputElement }) {
		const file = e.currentTarget.files?.[0];
		e.currentTarget.value = '';
		if (!file) return;
		apk = { name: file.name, sent: 0 };
		await g.run(
			() => androidTvApi.installApk(file, (sent) => apk && (apk.sent = sent)),
			() => {
				haptic(CONFIRM);
				ui.toast(m.android_tv_apk_installed({ name: file.name }));
				void oninstalled();
			},
			'apk'
		);
		apk = null;
		// the upload's progress gave way to the button again: the focus goes back to it
		await refocus(() => document.getElementById(`${id}-file`));
	}
</script>

{#if apk && apk.sent < 1}
	<Progress
		label={m.android_tv_uploading({ name: apk.name })}
		value={apk.sent * 100}
		max={100}
		valueText={m.android_tv_upload_sent({ percent: Math.round(apk.sent * 100) })}
	/>
{:else if apk}
	<p class="hint installing"><Icon name="upload" busy />{m.android_tv_installing()}</p>
{:else}
	<div class="actions">
		<label class="btn file">
			<Icon name="upload" />{m.android_tv_install_apk()}
			<input id="{id}-file" class="sr-only" type="file" accept=".apk,application/vnd.android.package-archive" onchange={install} />
		</label>
	</div>
{/if}

<style>
	.file {
		position: relative;
	}
	.file:focus-within {
		outline: var(--focus-ring);
		outline-offset: var(--focus-offset);
	}
	.installing {
		display: flex;
		align-items: center;
		gap: var(--s-1);
	}
</style>
