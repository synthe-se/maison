<script lang="ts">
	// One device on the dashboard (docs/ux/tableau-de-bord.md § 1): its icon is its main gesture
	// (a pressed button named after the device), the name and state lead to its page (one 44 px
	// target), the state is said in words on line 2, one short fact on the right, and at most
	// one control below. Unreachable (§ 4), the gesture stays where it is, said unavailable
	// with its reason (the state line), rather than vanishing.
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
		/** The device cannot be reached: the gesture is shown, not operable, the state line
		 * saying why (« Injoignable depuis 3 min »). */
		offline?: boolean;
		/** One short fact on the right (« 42 W », « 21 °C »). */
		fact?: string;
		/** 3 on the dashboard (under its group's h2), 2 alone on a device's page (under the h1). */
		level?: 2 | 3;
		/** Buttons at the end of the head row (settings…). */
		end?: Snippet;
		children?: Snippet;
	}
	let { name, icon, state, href, on, command, ontoggle, warn = false, offline = false, fact, level = 3, end, children }: Props = $props();

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
				class:on={shown && !offline}
				class:inflight={command?.target !== undefined}
				class:slow={command?.slow}
				aria-pressed={shown && !offline}
				aria-label={name}
				aria-disabled={offline ? 'true' : undefined}
				aria-describedby={offline ? `${id}-state` : undefined}
				onclick={() => !offline && ontoggle(!shown)}
			>
				<Icon name={icon} size={22} />
			</button>
		{:else}
			<span class="glyph" class:on aria-hidden="true"><Icon name={icon} size={22} /></span>
		{/if}
		<div class="text" class:linked={!!href}>
			<svelte:element this={`h${level}`} class="tile-title" id="{id}-name">
				<!-- the link covers the name and the state line (::after): one big target -->
				{#if href}<a class="tile-link" {href} aria-describedby="{id}-state">{name}</a>{:else}{name}{/if}
			</svelte:element>
			<p class="tile-state" id="{id}-state" class:warn={warn || offline || command?.late}>
				{line}
				{#if command?.late && ontoggle && on !== undefined && !offline}
					· <button class="link-btn retry" onclick={() => ontoggle(!on)}>{m.common_retry()}</button>
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
	/* a disc: on, filled in the accent; off, an empty 1.5 px ring in --ink-muted (≥ 3:1 in both
	   themes, WCAG 1.4.11) — two shapes, not colour alone (§ 1) */
	.glyph.gesture { border: 1.5px solid var(--ink-muted); border-radius: 50%; cursor: pointer; padding: 0; }
	.glyph.gesture:hover { border-color: var(--accent); }
	.glyph.gesture.on { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
	.glyph.gesture:not(.on) { background: transparent; color: var(--ink-muted); }
	.glyph.gesture.inflight { border-style: dashed; border-color: var(--accent); }
	/* unreachable: the struck circle of § 4, in the down colour */
	.glyph.gesture[aria-disabled='true'] { cursor: not-allowed; border-color: var(--status-down); color: var(--status-down); }
	.glyph.gesture.slow { animation: ring 1.2s ease-in-out infinite; }
	@keyframes ring { 50% { box-shadow: 0 0 0 4px var(--accent-wash); } }
	.fact { font: var(--t-meta); color: var(--ink-muted); white-space: nowrap; font-variant-numeric: tabular-nums; }
	/* the name and its state line: one link, at least 44 px tall */
	.text.linked { position: relative; min-height: var(--control-h); display: flex; flex-direction: column; justify-content: center; }
	.tile-link { font: inherit; color: var(--ink); text-decoration: underline; text-decoration-color: var(--accent-soft); text-underline-offset: 3px; }
	.tile-link:hover { text-decoration-color: currentColor; }
	.tile-link::after { content: ''; position: absolute; inset: 0; border-radius: var(--radius-m); }
	.tile-link:focus-visible { outline: none; }
	.tile-link:focus-visible::after { outline: 3px solid var(--accent); outline-offset: 2px; }
	/* « Réessayer » sits above the link's area */
	.retry { position: relative; z-index: 1; }
</style>
