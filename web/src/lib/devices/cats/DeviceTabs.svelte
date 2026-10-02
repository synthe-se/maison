<script lang="ts">
	// Two tabs on a device page: its controls, then its schedule or its settings.
	import type { Snippet } from 'svelte';
	import Tabs from '#lib/components/Tabs.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { IconName } from '#lib/components/Icon.svelte';

	interface Props {
		/** The second tab's name and icon. */
		other: { label: string; icon: IconName };
		controls: Snippet;
		second: Snippet;
	}
	let { other, controls, second }: Props = $props();
	let value = $state('controls');
</script>

<Tabs
	tabs={[
		{ value: 'controls', label: m.device_tab_control(), icon: 'play' },
		{ value: 'second', label: other.label, icon: other.icon }
	]}
	bind:value
>
	{#snippet panel(v)}{@render (v === 'controls' ? controls : second)()}{/snippet}
</Tabs>
