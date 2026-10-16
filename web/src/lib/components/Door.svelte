<script lang="ts">
	// A door page's head and body, said once (signing in, an invitation, a wall screen pairing,
	// « the house does not answer »; docs/ux.md « Layout »): what PageHead is to the other pages.
	// The window title, a section named by its h1 (the one the focus goes to once in), a mark
	// above it when there is one, then a lead, one big action and their hints, stacked.
	import type { Snippet } from 'svelte';
	import { pageTitle } from '#lib/i18n.svelte.ts';

	interface Props {
		title: string;
		/** Drawn above the title (an invitation accepted: its tick). */
		mark?: Snippet;
		/** The h1, to give it the focus. */
		heading?: HTMLElement;
		children?: Snippet;
	}
	let { title, mark, heading = $bindable(), children }: Props = $props();
	const id = $props.id();
</script>

<svelte:head><title>{pageTitle(title)}</title></svelte:head>

<section class="door" aria-labelledby={id}>
	{@render mark?.()}
	<h1 {id} tabindex="-1" bind:this={heading}>{title}</h1>
	{@render children?.()}
</section>

<style>
	.door {
		display: grid;
		gap: var(--s-4);
	}
	.door :global(p) {
		margin: 0;
	}
</style>
