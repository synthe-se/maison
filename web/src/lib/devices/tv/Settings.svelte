<script lang="ts">
	// A remote's settings, unfolded inside its tile: its addresses, saved together; whatever
	// else the device needs (pairing, an APK) comes as children. A refusal is said under the
	// fields, the focus back on the first one (not only in a toast).
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';

	export type Field = { key: string; label: string; placeholder: string };

	interface Props {
		fields: Field[];
		/** The saved values, read once when the settings open. */
		initial: Record<string, string | null | undefined>;
		/** Shown while nothing is configured yet. */
		hint?: string;
		/** Sends the values (empty ones as null) and re-reads the device. */
		save: (values: Record<string, string | null>) => Promise<unknown>;
		/** The confirmation, for a toast. */
		saved: string;
		onsaved?: () => void;
		children?: Snippet;
	}
	let { fields, initial, hint, save, saved, onsaved, children }: Props = $props();
	const id = $props.id();

	// a draft: a poll that lands while typing must not overwrite it
	// svelte-ignore state_referenced_locally
	let draft = $state(Object.fromEntries(fields.map((f) => [f.key, initial[f.key] ?? ''])));
	const saving = new Gesture();
	let first = $state<HTMLInputElement>();

	function submit(e: SubmitEvent) {
		e.preventDefault();
		return saving.run(
			() => save(Object.fromEntries(Object.entries(draft).map(([k, v]) => [k, v.trim() || null]))),
			() => {
				ui.toast(saved);
				onsaved?.();
			},
			'save',
			{ field: () => first }
		);
	}
</script>

<div class="inset">
	{#if hint}<p class="hint">{hint}</p>{/if}
	<form class="fields" onsubmit={submit}>
		{#each fields as f, i (f.key)}
			<div class="field">
				<label for="{id}-{f.key}">{f.label}</label>
				{#if i === 0}
					<input
						id="{id}-{f.key}"
						bind:this={first}
						bind:value={draft[f.key]}
						placeholder={f.placeholder}
						autocomplete="off"
						autocapitalize="off"
						spellcheck="false"
						aria-invalid={saving.error ? 'true' : undefined}
						aria-describedby="{id}-error"
					/>
				{:else}
					<input id="{id}-{f.key}" bind:value={draft[f.key]} placeholder={f.placeholder} autocomplete="off" autocapitalize="off" spellcheck="false" aria-describedby="{id}-error" />
				{/if}
			</div>
		{/each}
		<p class="form-error" id="{id}-error">{saving.error}</p>
		<div class="actions">
			<button class="btn primary" {...pending(saving.is())}>
				<Icon name="save" busy={saving.is()} />{m.common_save()}
			</button>
		</div>
	</form>
	{@render children?.()}
</div>

<style>
	.fields { display: grid; gap: var(--s-3); grid-template-columns: repeat(auto-fill, minmax(min(12rem, 100%), 1fr)); }
	.fields .actions, .fields .form-error { grid-column: 1 / -1; }
	.fields input { min-width: 0; }
</style>
