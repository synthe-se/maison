<script lang="ts">
	// The door, before the house: a centred 25rem card with the brand, no header to align with
	// (docs/ux/mise-en-page.md § 3). Signing in and an invitation share it (Ariane's AuthShell).
	import type { Snippet } from 'svelte';
	import Brand from './Brand.svelte';

	let { children }: { children: Snippet } = $props();
</script>

<div class="card">
	<Brand />
	{@render children()}
</div>

<style>
	.card { width: min(25rem, calc(100% - 2 * var(--page-margin))); margin: 12vh auto 0; display: grid; gap: var(--s-3); }
	.card :global(h1) { font: var(--t-page); margin: var(--s-5) 0 0; }
	.card :global(p) { margin: 0; }
	.card :global(.big) { min-height: var(--control-h); justify-content: center; margin-top: var(--s-3); }
	.card :global(.aside) { padding-top: var(--s-4); border-top: 1px solid var(--line); }
</style>
