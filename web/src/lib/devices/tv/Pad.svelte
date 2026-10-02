<script lang="ts">
	// The directional pad both remotes share (docs/ux/tableau-de-bord.md § 7): four 56 px arrows
	// around a 64 px OK, 8 px apart, the keys under it at 48 px; 192 px wide, inside one hand's
	// 216. Keyboard shortcuts only while the pad itself has focus (WCAG 2.1.4): arrows, Enter = OK,
	// Backspace = Back, + / − volume, M mute; listed in <kbd> from 840 px.
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import Key from './Key.svelte';
	import { repeatEvery, type Fire, type PadKey } from './remote.ts';

	interface Props {
		/** The pad's name (« Pavé de Télévision »). */
		label: string;
		keys: Record<PadKey, Fire>;
		/** The 48 px keys under the pad, in order. */
		under: PadKey[];
		/** Route + / − / M to the volume keys. */
		volume?: boolean;
		disabled?: boolean;
		/** More 48 px keys at the end of the row under the pad (the TV's Source…). */
		extra?: Snippet;
	}
	let { label, keys, under, volume = false, disabled = false, extra }: Props = $props();
	const id = $props.id();

	const SHORTCUT: Record<string, PadKey> = {
		ArrowUp: 'up',
		ArrowDown: 'down',
		ArrowLeft: 'left',
		ArrowRight: 'right',
		Enter: 'ok',
		Backspace: 'back'
	};
	const VOLUME_SHORTCUT: Record<string, PadKey> = { '+': 'volume_up', '=': 'volume_up', '-': 'volume_down', m: 'mute', M: 'mute' };

	// a held keyboard key repeats at the OS rate (~30 a second): kept to the pad's own pace
	const lastAt: Partial<Record<PadKey, number>> = {};

	function onkeydown(e: KeyboardEvent) {
		// only the pad itself: on a focused key, Enter and Space keep activating that key
		if (disabled || e.target !== e.currentTarget || e.altKey || e.ctrlKey || e.metaKey) return;
		const k = SHORTCUT[e.key] ?? (volume ? VOLUME_SHORTCUT[e.key] : undefined);
		if (!k) return;
		e.preventDefault();
		const every = repeatEvery(k);
		if (e.repeat && (every === 0 || Date.now() - (lastAt[k] ?? 0) < every)) return;
		lastAt[k] = Date.now();
		void keys[k](e.repeat);
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions: the pad takes focus to receive its shortcuts (2.1.4) -->
<div class="pad-wrap" role="group" aria-label={label} aria-describedby="{id}-kbd" tabindex="0" {onkeydown}>
	<div class="pad">
		<span class="up"><Key k="up" fire={keys.up} {disabled} /></span>
		<span class="left"><Key k="left" fire={keys.left} {disabled} /></span>
		<span class="ok"><Key k="ok" fire={keys.ok} size="ok" {disabled} /></span>
		<span class="right"><Key k="right" fire={keys.right} {disabled} /></span>
		<span class="down"><Key k="down" fire={keys.down} {disabled} /></span>
	</div>
	<div class="under">
		{#each under as k (k)}<Key {k} fire={keys[k]} size="small" {disabled} />{/each}
		{@render extra?.()}
	</div>
	<dl class="kbd" id="{id}-kbd">
		<div><dt><kbd>{m.tv_kbd_arrows()}</kbd></dt><dd>{m.tv_kbd_move()}</dd></div>
		<div><dt><kbd>{m.tv_kbd_enter()}</kbd></dt><dd>{m.tv_key_ok()}</dd></div>
		<div><dt><kbd>{m.tv_kbd_backspace()}</kbd></dt><dd>{m.tv_key_back()}</dd></div>
		{#if volume}
			<div><dt><kbd>{m.tv_kbd_plus_minus()}</kbd></dt><dd>{m.tv_volume()}</dd></div>
			<div><dt><kbd>{m.tv_kbd_m()}</kbd></dt><dd>{m.tv_mute()}</dd></div>
		{/if}
	</dl>
</div>

<style>
	.pad-wrap { display: grid; justify-items: center; gap: var(--s-3); padding: var(--s-2); border-radius: var(--radius-l); }
	.pad-wrap:focus-visible { outline: 3px solid var(--accent); outline-offset: 2px; }
	.pad {
		display: grid; gap: var(--s-2); place-items: center;
		grid-template-columns: var(--remote-key) var(--remote-ok) var(--remote-key);
		grid-template-areas: '. up .' 'left ok right' '. down .';
	}
	.up { grid-area: up; }
	.left { grid-area: left; }
	.ok { grid-area: ok; }
	.right { grid-area: right; }
	.down { grid-area: down; }
	.under { display: flex; flex-wrap: wrap; justify-content: center; gap: var(--s-2); }
	/* the shortcuts are for a keyboard: shown from 840 px, always read with the pad (describedby) */
	.kbd { display: none; margin: 0; font: var(--t-secondary); color: var(--ink-muted); }
	.kbd div { display: inline-flex; gap: var(--s-1); margin-inline: var(--s-2); }
	.kbd dd { margin: 0; }
	@media (min-width: 840px) {
		.kbd { display: flex; flex-wrap: wrap; justify-content: center; }
	}
</style>
