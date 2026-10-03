<script lang="ts">
	// A tile's settings, unfolded inside it under its gear (DeviceTile `settings`: a Bits UI
	// Collapsible, the gear its trigger, so `aria-expanded` and `aria-controls` come with it).
	// What is inside exists only while open (a form starts afresh each time). Closed while the
	// focus was in them (a form saved, a field cancelled), the focus goes back to the gear that
	// opened them (WCAG 2.4.3).
	import type { Snippet } from 'svelte';
	import { Collapsible } from 'bits-ui';
	import { refocus } from '#lib/focus.ts';

	interface Props {
		open: boolean;
		/** The gear: where the focus returns. */
		trigger: () => HTMLElement | null | undefined;
		children: Snippet;
	}
	let { open, trigger, children }: Props = $props();
	let content = $state<HTMLElement | null>(null);

	// before the DOM drops them: was the focus inside?
	$effect.pre(() => {
		if (!open && content?.contains(document.activeElement)) void refocus(trigger);
	});
</script>

<Collapsible.Content bind:ref={content}>
	{#snippet child({ props, open: shown })}
		<div {...props} class="inset tile-settings">
			{#if shown}{@render children()}{/if}
		</div>
	{/snippet}
</Collapsible.Content>

<style>
	.tile-settings {
		gap: var(--s-4);
	}
</style>
