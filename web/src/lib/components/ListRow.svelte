<script lang="ts">
	// One row of a list of things one manages (a passkey, a person, an invitation): its name
	// and what it carries (chips), a line of facts under it, its actions at the end; `whole`
	// replaces the row's content (a name edited in place). app.css `.list-row`.
	import type { Snippet } from 'svelte';

	interface Props {
		/** The name, and what goes with it (« Synchronisée »). */
		children?: Snippet;
		/** The facts under it (« Créée le 2 octobre 2026 · Jamais utilisée »). */
		second?: string;
		/** Its actions. */
		end?: Snippet;
		/** In place of the row's content. */
		whole?: Snippet;
	}
	let { children, second, end, whole }: Props = $props();
</script>

<li class="list-row">
	{#if whole}
		<div class="whole">{@render whole()}</div>
	{:else}
		<span class="title">{@render children?.()}</span>
		{#if second}<span class="second">{second}</span>{/if}
		{#if end}<span class="end">{@render end()}</span>{/if}
	{/if}
</li>
