<script lang="ts">
	// One device on the dashboard (docs/ux.md § 1): its icon is its main gesture
	// (a pressed button named after the device), the name and state lead to its page (one 44 px
	// target), the state is said in words on line 2, one short fact on the right, and at most
	// one control below. Unreachable (§ 4), the gesture stays where it is, said unavailable
	// with its reason (the state line), rather than vanishing.
	import type { Snippet } from 'svelte';
	import { Collapsible } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import type { Command } from '#lib/command.svelte.ts';
	import { unavailable } from '#lib/gesture.svelte.ts';
	import Icon, { type IconName } from './Icon.svelte';
	import TileSettings from './TileSettings.svelte';

	interface Props {
		name: string;
		icon?: IconName;
		/** Line 2: the state in words (« Allumée, 80 % », « Injoignable depuis 3 min »). */
		state: string;
		/** The device's page, if it has one. */
		href?: string;
		/** On/off as read from the device; with `command` the icon becomes the toggle. */
		on?: boolean;
		command?: Command;
		ontoggle?: (next: boolean) => void;
		/** The state line in the warning color (unreachable, no answer). */
		warn?: boolean;
		/** The device cannot be reached: the gesture is shown, not operable, the state line
		 * saying why (« Injoignable depuis 3 min »). */
		offline?: boolean;
		/** One short fact on the right (« 42 W », « 21 °C »). */
		fact?: string;
		/** In place of the icon, when the tile has no gesture (Tempo's swatch). */
		lead?: Snippet;
		/** 3 on the dashboard (under its group's h2), 2 alone on a device's page (under the h1). */
		level?: 2 | 3;
		/** Buttons at the end of the head row. Each snippet gets the state line's id: what a control
		 * that cannot act is described by (`unavailable(why)`, § 4). */
		end?: Snippet<[why: string]>;
		children?: Snippet<[why: string]>;
		/** The tile's settings, under a gear at the end of the head row (TileSettings). */
		settings?: Snippet;
		settingsOpen?: boolean;
		/** The state line's id, when controls outside the tile are described by it (a page's pad). */
		stateId?: string;
	}
	let {
		name,
		icon,
		state: said,
		href,
		on,
		command,
		ontoggle,
		warn = false,
		offline = false,
		fact,
		lead,
		level = 3,
		end,
		children,
		settings,
		settingsOpen = $bindable(false),
		stateId
	}: Props = $props();

	const id = $props.id();
	// svelte-ignore state_referenced_locally (an id, fixed for the tile's life)
	const why = stateId ?? `${id}-state`;
	let gear = $state<HTMLElement | null>(null);
	const shown = $derived(command && on !== undefined ? command.shown(on) : on);
	const line = $derived(
		command?.late ? m.command_no_answer_short() : command?.slow ? (command.target ? m.command_turning_on() : m.command_turning_off()) : said
	);
</script>

{#if settings}
	<Collapsible.Root bind:open={settingsOpen}>
		{#snippet child()}{@render tile()}{/snippet}
	</Collapsible.Root>
{:else}
	{@render tile()}
{/if}

{#snippet tile()}
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
					{...unavailable(offline && why)}
					onclick={() => !offline && ontoggle(!shown)}
				>
					<Icon name={icon ?? 'circle'} size={22} />
				</button>
			{:else if lead}
				{@render lead()}
			{:else}
				<span class="glyph" class:on aria-hidden="true"><Icon name={icon ?? 'circle'} size={22} /></span>
			{/if}
			<div class="text" class:linked={!!href}>
				<svelte:element this={`h${level}`} class="tile-title" id="{id}-name">
					<!-- the link covers the name and the state line (::after): one big target -->
					{#if href}<a class="tile-link" {href} aria-describedby={why}>{name}</a>{:else}{name}{/if}
				</svelte:element>
				<p class="tile-state" id={why} class:warn={warn || offline || command?.late}>
					{line}
					{#if command?.late && ontoggle && on !== undefined && !offline}
						· <button class="link-btn retry" onclick={() => ontoggle(!on)}>{m.common_retry()}</button>
					{/if}
				</p>
			</div>
			{#if fact || end || settings}
				<div class="end">
					{#if fact}<span class="fact">{fact}</span>{/if}
					{@render end?.(why)}
					{#if settings}
						<Collapsible.Trigger class="icon-btn" aria-label={m.common_settings_of({ name })} bind:ref={gear}>
							<Icon name="settings-2" />
						</Collapsible.Trigger>
					{/if}
				</div>
			{/if}
		</div>
		{@render children?.(why)}
		{#if settings}<TileSettings open={settingsOpen} trigger={() => gear}>{@render settings()}</TileSettings>{/if}
	</article>
{/snippet}

<style>
	/* a disc: on, filled in the accent; off, an empty 1.5 px ring in --ink-muted (≥ 3:1 in both
	   themes, WCAG 1.4.11) — two shapes, not color alone (§ 1) */
	.glyph.gesture {
		border: 1.5px solid var(--ink-muted);
		border-radius: 50%;
		cursor: pointer;
		padding: 0;
	}
	.glyph.gesture:hover {
		border-color: var(--accent);
	}
	.glyph.gesture.on {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--on-accent);
	}
	.glyph.gesture:not(.on) {
		background: transparent;
		color: var(--ink-muted);
	}
	.glyph.gesture.inflight {
		border-style: dashed;
		border-color: var(--accent);
	}
	/* unreachable: the struck circle of § 4, in the down color */
	.glyph.gesture[aria-disabled='true'] {
		cursor: not-allowed;
		border-color: var(--status-down);
		color: var(--status-down);
	}
	.glyph.gesture.slow {
		animation: ring 1.2s ease-in-out infinite;
	}
	@keyframes ring {
		50% {
			box-shadow: 0 0 0 var(--s-1) var(--accent-wash);
		}
	}
	.fact {
		font: var(--t-meta);
		color: var(--ink-muted);
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}
	/* the name and its state line: one link, at least 44 px tall */
	.text.linked {
		position: relative;
		min-height: var(--control-h);
		display: flex;
		flex-direction: column;
		justify-content: center;
	}
	.tile-link {
		font: inherit;
		color: var(--ink);
		text-decoration: underline;
		text-decoration-color: var(--accent-soft);
		text-underline-offset: var(--s-1);
	}
	.tile-link:hover {
		text-decoration-color: currentColor;
	}
	.tile-link::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: var(--radius-m);
	}
	.tile-link:focus-visible {
		outline: none;
	}
	.tile-link:focus-visible::after {
		outline: var(--focus-ring);
		outline-offset: var(--focus-offset);
	}
	/* « Réessayer » sits above the link's area */
	.retry {
		position: relative;
		z-index: 1;
	}
</style>
