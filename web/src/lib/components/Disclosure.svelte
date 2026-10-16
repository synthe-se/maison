<script lang="ts">
	// Something folded under its name, for whoever wants it (Fold: a button that says whether it
	// is open, `aria-controls` its content): the AC's raw infrared order. What is inside exists
	// only while open.
	import type { Snippet } from 'svelte';
	import Fold from './Fold.svelte';
	import Icon from './Icon.svelte';

	let { label, open = $bindable(false), children: content }: { label: string; open?: boolean; children: Snippet } = $props();
</script>

{#snippet name()}<Icon name={open ? 'chevron-up' : 'chevron-down'} size={16} />{label}{/snippet}

<Fold bind:open>
	{#snippet children(trigger, panel)}
		<div class="disclosure">
			{@render trigger({ class: 'link-btn quiet disclosure-trigger', body: name })}
			{@render panel({ body: content })}
		</div>
	{/snippet}
</Fold>

<style>
	.disclosure {
		display: grid;
		gap: var(--s-2);
	}
	:global(.disclosure-trigger) {
		display: inline-flex;
		align-items: center;
		gap: var(--s-1);
		min-height: var(--control-h-xs);
		font: var(--t-secondary);
		text-decoration: none;
	}
</style>
