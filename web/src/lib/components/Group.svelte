<script lang="ts">
	// A group, on the dashboard or a page (docs/ux.md § 5): its section named by its title (an
	// h2 that can take the focus: a « Maintenant » chip leads to it by `id`, a removed row gives
	// it the focus back), one short fact, its actions, then what it holds. A value read late is
	// said once under the head (`value`, for a group that shows it without Loaded).
	import type { Snippet } from 'svelte';
	import type { Loadable } from '#lib/live.svelte.ts';
	import Stale from './Stale.svelte';

	interface Props {
		/** The title's id: what the section is labelled by, and where a chip leads. */
		id: string;
		title: string;
		/** One short fact on the right (« 3 appareils »). */
		fact?: string;
		/** Buttons at the end of the head (add, « Tout éteindre »…). */
		actions?: Snippet;
		value?: Loadable;
		/** The title element, to give it the focus. */
		heading?: HTMLElement;
		children: Snippet;
	}
	let { id, title, fact, actions, value, heading = $bindable(), children }: Props = $props();
</script>

<section class="group" aria-labelledby={id}>
	<div class="group-head">
		<h2 {id} class="group-title" tabindex="-1" bind:this={heading}>{title}</h2>
		{#if fact}<span class="fact">{fact}</span>{/if}
		{#if actions}<div class="actions">{@render actions()}</div>{/if}
	</div>
	{#if value}<Stale {value} />{/if}
	{@render children()}
</section>
