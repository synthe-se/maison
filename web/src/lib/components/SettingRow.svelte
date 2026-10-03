<script lang="ts">
	// A settings row (docs/ux.md § 2, case 2): an icon, the label on the left,
	// the switch on the right (role=switch, only in such lists), an optional line under.
	// Put rows in a `<ul class="settings">`.
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import Toggle from '#lib/components/Toggle.svelte';

	interface Props {
		icon: IconName;
		label: string;
		checked: boolean;
		onchange: (next: boolean) => void;
		busy?: boolean;
		/** The id of what says why it cannot act now (Toggle). */
		reason?: string;
		hint?: string;
	}
	let { icon, label, checked, onchange, busy = false, reason, hint }: Props = $props();
</script>

<li class="setting">
	<span class="lead" aria-hidden="true"><Icon name={icon} /></span>
	<Toggle {label} {checked} {onchange} {busy} {reason} />
	{#if hint}<p class="hint">{hint}</p>{/if}
</li>

<style>
	.setting {
		display: grid;
		grid-template-columns: var(--lead) minmax(0, 1fr);
		align-items: center;
		gap: 0 var(--s-3);
		min-height: var(--row-min);
		padding-block: var(--s-1);
		border-bottom: 1px solid var(--line);
	}
	.setting:last-child {
		border-bottom: 0;
	}
	.lead {
		display: grid;
		place-items: center;
		color: var(--ink-muted);
	}
	.setting :global(.toggle) {
		display: flex;
		justify-content: space-between;
		width: 100%;
	}
	.hint {
		grid-column: 2;
		padding-bottom: var(--s-2);
	}
</style>
