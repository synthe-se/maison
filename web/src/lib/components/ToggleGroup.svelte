<script lang="ts" generics="T extends string">
	// A few choices side by side, pressed in place (Bits UI ToggleGroup, APG radio group or
	// toggle buttons, one Tab stop and arrows between them): one of them (`single`: a mode, the
	// language, the theme), or any of them (`multiple`: a meal's days). Busy, the choices keep
	// the focus and ignore presses (§ 2); a single choice is never emptied by pressing it again.
	import type { Snippet } from 'svelte';
	import { ToggleGroup } from 'bits-ui';
	import { pending, unavailable } from '#lib/gesture.svelte.ts';

	type Choice = { value: T; label: string; lang?: string };
	type Props = {
		label: string;
		/** The context says it already; it stays for readers. */
		hideLabel?: boolean;
		options: readonly Choice[];
		/** An order in flight: busy, not operable, still focusable. */
		busy?: boolean;
		/** The id of what says why it cannot act now. */
		reason?: string;
		/** The id of what describes the group (a problem with the choice). */
		describedby?: string;
		/** A choice's own content (a short day name shown, the long one read). */
		item?: Snippet<[Choice]>;
	} & ({ type: 'single'; value: T | undefined; onchange: (v: T) => void } | { type: 'multiple'; value: T[]; onchange: (v: T[]) => void });

	let props: Props = $props();
	const id = $props.id();
	const locked = $derived(props.busy || !!props.reason);
</script>

<div class="choices">
	<span class="label" class:sr-only={props.hideLabel} id="{id}-label">{props.label}</span>
	{#if props.type === 'single'}
		{@const p = props}
		<ToggleGroup.Root
			type="single"
			class="segmented"
			aria-labelledby="{id}-label"
			aria-describedby={props.describedby}
			bind:value={() => p.value ?? '', (v) => v && !locked && p.onchange(v as T)}
		>
			{@render items()}
		</ToggleGroup.Root>
	{:else}
		{@const p = props}
		<ToggleGroup.Root
			type="multiple"
			class="segmented"
			aria-labelledby="{id}-label"
			aria-describedby={props.describedby}
			bind:value={() => p.value, (v) => !locked && p.onchange(v as T[])}
		>
			{@render items()}
		</ToggleGroup.Root>
	{/if}
</div>

{#snippet items()}
	{#each props.options as o (o.value)}
		<ToggleGroup.Item class="btn" value={o.value} lang={o.lang} {...pending(!!props.busy)} {...unavailable(props.reason)}>
			{#if props.item}{@render props.item(o)}{:else}{o.label}{/if}
		</ToggleGroup.Item>
	{/each}
{/snippet}

<style>
	.choices {
		display: grid;
		gap: var(--s-2);
	}
	.choices :global(.segmented) {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(0, 1fr);
		gap: var(--s-1);
	}
	.choices :global(.segmented .btn) {
		justify-content: center;
		min-height: var(--control-h);
		padding-inline: var(--s-2);
		white-space: normal;
		text-align: center;
	}
	.choices :global(.segmented .btn[data-state='on']) {
		background: var(--accent);
		color: var(--on-accent);
		border-color: var(--accent);
		font-weight: 600;
	}
</style>
