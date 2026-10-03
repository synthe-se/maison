<script lang="ts">
	// Views of one thing, side by side (APG tabs, through Bits UI): a device's controls and its
	// schedule, a lamp's white and its color. `panel` draws the chosen view.
	import type { Snippet } from 'svelte';
	import { Tabs } from 'bits-ui';
	import Icon, { type IconName } from './Icon.svelte';

	interface Props {
		tabs: readonly { value: string; label: string; icon?: IconName }[];
		value: string;
		onchange?: (value: string) => void;
		panel: Snippet<[string]>;
	}
	let { tabs, value = $bindable(), onchange, panel }: Props = $props();
</script>

<Tabs.Root
	{value}
	onValueChange={(v) => {
		value = v;
		onchange?.(v);
	}}
	class="tabs"
>
	<Tabs.List class="tabs-list">
		{#each tabs as t (t.value)}
			<Tabs.Trigger value={t.value} class="tabs-trigger"
				>{#if t.icon}<Icon name={t.icon} />{/if}{t.label}</Tabs.Trigger
			>
		{/each}
	</Tabs.List>
	{#each tabs as t (t.value)}
		<Tabs.Content value={t.value} class="tabs-panel">{@render panel(t.value)}</Tabs.Content>
	{/each}
</Tabs.Root>

<style>
	:global(.tabs) {
		display: grid;
		gap: var(--s-4);
	}
	:global(.tabs-panel) {
		display: grid;
		gap: var(--s-4);
	}
</style>
