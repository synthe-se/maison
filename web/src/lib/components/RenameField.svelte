<script lang="ts">
	// A name changed in place, the same everywhere (a shutter, a Zigbee lamp, a passkey): Entrée
	// saves, Échap gives up and the name it had comes back, the focus stays in the field, the
	// outcome is said once, a refusal is shown under the field (not only in a toast).
	import { m } from '#lib/paraglide/messages.js';
	import { MAX_NAME } from '#lib/api.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from './Icon.svelte';

	interface Props {
		label: string;
		/** The name as the device has it. */
		value: string;
		save: (name: string) => Promise<unknown>;
		/** Said once saved (« Renommé en Chambre »). */
		said: (name: string) => string;
		/** Inline in a row: the field closes on save or Escape; the caller puts the focus back. */
		ondone?: () => void;
		hideLabel?: boolean;
		/** Takes the focus when it appears (an inline rename just opened). */
		autofocus?: boolean;
	}
	let { label, value, save, said, ondone, hideLabel = false, autofocus = false }: Props = $props();
	const id = $props.id();
	const g = new Gesture();

	/** What is typed; null: the device's name. Kept after a save until a read shows it (a poll
	 * already in flight may still carry the old name). */
	let draft = $state<string | null>(null);
	const text = $derived(draft ?? value);
	let invalid = $state('');
	const problem = $derived(invalid || g.error);
	let input = $state<HTMLInputElement>();

	$effect(() => {
		if (draft !== null && draft === value && !g.is()) draft = null;
	});
	$effect(() => {
		if (autofocus) input?.focus();
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		const name = text.trim();
		invalid = '';
		if (!name) {
			invalid = m.rename_empty();
			input?.focus();
			return;
		}
		if (name === value) return ondone?.();
		let saved = false;
		await g.run(
			() => save(name),
			() => {
				saved = true;
				draft = name;
				ui.say(said(name));
			},
			'save',
			{ field: () => input }
		);
		if (saved) ondone?.();
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key !== 'Escape') return;
		// the field's own Escape: a sheet or a panel around it stays open
		e.preventDefault();
		e.stopPropagation();
		draft = null;
		invalid = g.error = '';
		ondone?.();
	}
</script>

<form class="field rename" onsubmit={submit} novalidate>
	<label for="{id}-name" class:sr-only={hideLabel}>{label}</label>
	<div class="row">
		<input
			id="{id}-name"
			bind:this={input}
			value={text}
			oninput={(e) => (draft = e.currentTarget.value)}
			{onkeydown}
			maxlength={MAX_NAME}
			autocomplete="off"
			aria-invalid={problem ? 'true' : undefined}
			aria-describedby={problem ? `${id}-error` : undefined}
		/>
		<button class="btn primary" {...pending(g.is('save'))}><Icon name="check" busy={g.is('save')} />{m.common_save()}</button>
		{#if ondone}<button class="btn ghost" type="button" onclick={() => ondone()}>{m.common_cancel()}</button>{/if}
	</div>
	<p class="form-error" id="{id}-error">{problem}</p>
</form>

<style>
	.row {
		display: flex;
		gap: var(--s-2);
		flex-wrap: wrap;
	}
	.row input {
		flex: 1 1 12rem;
		min-width: 0;
	}
	.row .btn {
		min-height: var(--control-h);
	}
</style>
