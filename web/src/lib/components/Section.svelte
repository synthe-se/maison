<script lang="ts">
	// One block of a device page: its title, then its rows (`ul.settings` of SettingRow,
	// `dl.facts` of StatusRow) and actions. Every device page is a column of these: framed as a
	// tile, or bare (`tile={false}`: the lamp's details under its tile); an h2, or an h3 inside a
	// block (`level={3}`). The title is where the focus can go back to (`bind:heading`).
	import type { Snippet } from 'svelte';
	import Icon, { type IconName } from './Icon.svelte';

	interface Props {
		title: string;
		icon?: IconName;
		tile?: boolean;
		level?: 2 | 3;
		heading?: HTMLElement;
		children: Snippet;
	}
	let { title, icon, tile = true, level = 2, heading = $bindable(), children }: Props = $props();
	const id = $props.id();
</script>

<section class="section" class:tile aria-labelledby={id}>
	<svelte:element this={`h${level}`} class={level === 2 ? 'group-title' : 'label'} {id} tabindex="-1" bind:this={heading}>
		{#if icon}<Icon name={icon} />{/if}{title}
	</svelte:element>
	{@render children()}
</section>

<style>
	.section {
		display: grid;
		gap: var(--s-3);
	}
	.section.tile {
		gap: var(--s-4);
	}
	.section > :first-child {
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
