<script lang="ts">
	// A value still shown but old (docs/ux.md § 4: the last value stays, said as such): the last
	// ask failed, or the polls have not answered for a while. Said once, where the value is (in
	// Loaded, or under a group's head), with the time it was read and a way to ask again. Not a
	// live region: a refresh is never announced.
	import { m } from '#lib/paraglide/messages.js';
	import type { Loadable } from '#lib/live.svelte.ts';
	import { when } from '#lib/i18n.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from './Icon.svelte';

	let { value }: { value: Loadable } = $props();
	const g = new Gesture();
</script>

{#if value.stale}
	<p class="hint stale">
		<Icon name="clock" size={14} />{m.live_stale({ time: when(value.at) })} ·
		<button class="link-btn" {...pending(g.is())} onclick={() => g.run(() => value.refresh())}>{m.common_retry()}</button>
	</p>
{/if}

<style>
	.stale {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--s-1);
	}
	.stale .link-btn {
		min-height: var(--control-h-xs);
	}
</style>
