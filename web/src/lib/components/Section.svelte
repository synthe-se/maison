<script lang="ts">
	// One block of a device page: its h2, then its rows (`ul.settings` of SettingRow,
	// `dl.facts` of StatusRow) and actions. Every cat device page is a column of these.
	import type { Snippet } from 'svelte';
	import Icon, { type IconName } from './Icon.svelte';

	let { title, icon, children }: { title: string; icon?: IconName; children: Snippet } = $props();
	const id = $props.id();
</script>

<section class="tile section" aria-labelledby={id}>
	<h2 class="group-title" {id}>
		{#if icon}<Icon name={icon} />{/if}{title}
	</h2>
	{@render children()}
</section>

<style>
	.section h2 {
		display: flex;
		align-items: center;
		gap: var(--s-2);
	}
	.section :global(.btn) {
		min-height: var(--control-h);
		justify-content: center;
	}
	.section :global(.wide) {
		width: 100%;
	}
</style>
