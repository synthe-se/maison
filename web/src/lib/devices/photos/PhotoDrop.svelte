<script lang="ts">
	// Adding photos: dropped here or picked, several at once. Long steps named and timed (§ 6): which
	// photo goes, how many are done; then the outcome once, what was refused in a warning that stays;
	// the focus goes back to the picker after.
	import { m } from '#lib/paraglide/messages.js';
	import { errorText } from '#lib/errors.ts';
	import { list } from '#lib/i18n.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Progress from '#lib/components/Progress.svelte';
	import { sendPhotos, type Sending, type Sent } from './send.ts';

	let { onadded }: { onadded: () => unknown } = $props();
	const id = $props.id();
	const g = new Gesture();
	let sending = $state<Sending | null>(null);
	/** Something is being dragged over the zone. */
	let over = $state(false);

	function outcome({ added, failed }: Sent) {
		if (added) {
			haptic(CONFIRM);
			ui.toast(m.photos_added({ count: added }));
		}
		// one warning per reason, however many photos it stopped
		const reasons = new Map<string, string[]>();
		for (const { name, error } of failed) {
			const why = errorText(error);
			reasons.set(why, [...(reasons.get(why) ?? []), name]);
		}
		for (const [why, names] of reasons) ui.toast(m.photos_not_added({ names: list(names), why }), { warn: true });
	}

	async function send(files: File[]) {
		if (!files.length || g.is('send')) return;
		await g.run(
			() => sendPhotos(files, (at) => (sending = at)),
			(sent) => {
				outcome(sent);
				return onadded();
			},
			'send'
		);
		sending = null;
		await refocus(() => document.getElementById(`${id}-file`));
	}

	function picked(e: Event & { currentTarget: HTMLInputElement }) {
		const files = [...(e.currentTarget.files ?? [])];
		e.currentTarget.value = '';
		void send(files);
	}

	function dropped(e: DragEvent) {
		e.preventDefault();
		over = false;
		void send([...(e.dataTransfer?.files ?? [])]);
	}
</script>

{#if sending}
	<Progress
		label={m.android_tv_uploading({ name: sending.name })}
		value={((sending.done + sending.sent) / sending.total) * 100}
		max={100}
		valueText={m.photos_progress({ done: sending.done + 1, total: sending.total })}
	/>
{:else}
	<div
		class="drop"
		class:over
		role="group"
		aria-labelledby="{id}-hint"
		ondragover={(e) => {
			e.preventDefault();
			over = true;
		}}
		ondragleave={() => (over = false)}
		ondrop={dropped}
	>
		<p id="{id}-hint" class="hint">{m.photos_drop()}</p>
		<label class="btn primary file">
			<Icon name="upload" />{m.photos_add()}
			<input id="{id}-file" class="sr-only" type="file" accept="image/*" multiple onchange={picked} />
		</label>
	</div>
{/if}

<style>
	.drop {
		display: grid;
		justify-items: start;
		gap: var(--s-3);
		padding: var(--s-4);
		border: 1.5px dashed var(--line);
		border-radius: var(--radius-m);
	}
	.drop.over {
		border-color: var(--accent);
		background: var(--accent-wash);
	}
	.drop p {
		margin: 0;
	}
	.file {
		position: relative;
	}
	.file:focus-within {
		outline: var(--focus-ring);
		outline-offset: var(--focus-offset);
	}
</style>
