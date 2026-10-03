<script lang="ts">
	// One fact read from a device, in a `<dl class="facts">`: label on the left, value in words
	// on the right (a device's state, a lamp's details); `warn` for what needs a hand (the word
	// says it, the color only repeats it).
	import Icon, { type IconName } from './Icon.svelte';

	interface Props {
		icon?: IconName;
		label: string;
		value: string;
		warn?: boolean;
	}
	let { icon, label, value, warn = false }: Props = $props();
</script>

<div class="fact-row" class:warn>
	<dt>
		{#if icon}<Icon name={icon} />{/if}{label}
	</dt>
	<dd>
		{#if warn}<Icon name="triangle-alert" />{/if}{value}
	</dd>
</div>

<style>
	.fact-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--s-3);
		min-height: var(--row-min);
		border-bottom: 1px solid var(--line);
	}
	.fact-row:last-child {
		border-bottom: 0;
	}
	dt {
		display: inline-flex;
		align-items: center;
		gap: var(--s-2);
		color: var(--ink-muted);
	}
	dd {
		margin: 0;
		display: inline-flex;
		align-items: center;
		gap: var(--s-1);
		font-weight: 600;
		text-align: right;
		overflow-wrap: anywhere;
		font-variant-numeric: tabular-nums;
	}
	.warn dd {
		color: var(--warn-text);
	}
</style>
