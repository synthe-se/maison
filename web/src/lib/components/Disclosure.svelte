<script lang="ts">
	// Something folded under its name, for whoever wants it (Bits UI Collapsible, APG
	// disclosure: a button that says whether it is open, `aria-controls` its content): the AC's
	// raw infrared order. What is inside exists only while open.
	import type { Snippet } from 'svelte';
	import { Collapsible } from 'bits-ui';
	import Icon from './Icon.svelte';

	let { label, open = $bindable(false), children }: { label: string; open?: boolean; children: Snippet } = $props();
</script>

<Collapsible.Root bind:open class="disclosure">
	<Collapsible.Trigger class="link-btn quiet disclosure-trigger">
		<Icon name={open ? 'chevron-up' : 'chevron-down'} size={16} />{label}
	</Collapsible.Trigger>
	<Collapsible.Content>
		{#snippet child({ props, open: shown })}
			<div {...props}>
				{#if shown}{@render children()}{/if}
			</div>
		{/snippet}
	</Collapsible.Content>
</Collapsible.Root>

<style>
	:global(.disclosure) {
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
