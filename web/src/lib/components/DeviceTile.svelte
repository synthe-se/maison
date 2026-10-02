<script lang="ts">
	// One device on the dashboard (docs/ux/tableau-de-bord.md § 1): its icon is its main gesture
	// (a pressed button named after the device), the name leads to its page, the state is said
	// in words on line 2, one short fact on the right, and at most one control below.
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { Command } from '#lib/command.svelte.ts';
	import Icon, { type IconName } from './Icon.svelte';

	interface Props {
		name: string;
		icon: IconName;
		/** Line 2: the state in words (« Allumée, 80 % », « Injoignable depuis 3 min »). */
		state: string;
		/** The device's page, if it has one. */
		href?: string;
		/** On/off as read from the device; with `command` the icon becomes the toggle. */
		on?: boolean;
		command?: Command;
		ontoggle?: (next: boolean) => void;
		/** The state line in the warning colour (unreachable, no answer). */
		warn?: boolean;
		/** One short fact on the right (« 42 W », « 21 °C »). */
		fact?: string;
		/** 3 on the dashboard (under its group's h2), 2 alone on a device's page (under the h1). */
		level?: 2 | 3;
		/** Buttons at the end of the head row (settings…). */
		end?: Snippet;
		children?: Snippet;
	}
	let { name, icon, state, href, on, command, ontoggle, warn = false, fact, level = 3, end, children }: Props = $props();

	const id = $props.id();
	const shown = $derived(command && on !== undefined ? command.shown(on) : on);
	const line = $derived(
		command?.late ? m.command_no_answer_short() : command?.slow ? (command.target ? m.command_turning_on() : m.command_turning_off()) : state
	);
</script>

<article class="tile" aria-labelledby="{id}-name">
	<div class="tile-head">
		{#if ontoggle && on !== undefined}
			<button
				class="glyph gesture"
				class:on={shown}
				class:inflight={command?.target !== undefined}
				class:slow={command?.slow}
				aria-pressed={shown}
				aria-label={name}
				onclick={() => ontoggle(!shown)}
			>
				<Icon name={icon} size={22} />
			</button>
		{:else}
			<span class="glyph" class:on aria-hidden="true"><Icon name={icon} size={22} /></span>
		{/if}
		<div class="text">
			<svelte:element this={`h${level}`} class="tile-title" id="{id}-name">
				{#if href}<a class="link-btn plain" {href}>{name}</a>{:else}{name}{/if}
			</svelte:element>
			<p class="tile-state" class:warn={warn || command?.late}>
				{line}
				{#if command?.late && ontoggle && on !== undefined}
					· <button class="link-btn" onclick={() => ontoggle(!on)}>{m.common_retry()}</button>
				{/if}
			</p>
		</div>
		{#if fact || end}
			<div class="end">
				{#if fact}<span class="fact">{fact}</span>{/if}
				{@render end?.()}
			</div>
		{/if}
	</div>
	{@render children?.()}
</article>

<style>
	.gesture { border: 2px solid transparent; cursor: pointer; padding: 0; }
	.gesture:hover { border-color: var(--accent-soft); }
	/* on: filled; off: an empty ring — shape, not colour alone */
	.gesture.on { background: var(--accent); color: var(--on-accent); }
	.gesture:not(.on) { background: transparent; border-color: var(--line); }
	.gesture.inflight { border-style: dashed; border-color: var(--accent); }
	.gesture.slow { animation: ring 1.2s ease-in-out infinite; }
	@keyframes ring { 50% { box-shadow: 0 0 0 4px var(--accent-wash); } }
	.fact { font: var(--t-meta); color: var(--ink-muted); white-space: nowrap; font-variant-numeric: tabular-nums; }
	.tile-title a { font: inherit; }
</style>
