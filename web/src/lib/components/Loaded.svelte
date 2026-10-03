<script lang="ts" generics="T">
	// What a live value shows before its content, the same in every group and page: the first
	// load (a skeleton of the final height, or a line), a failure with nothing known yet (said
	// as such, with a retry — never an empty list), nothing to show (why, and what to do), then
	// the content. No live region here: a refresh is never announced (§ 4).
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { Live } from '#lib/live.svelte.ts';
	import { errorText } from '#lib/errors.ts';
	import { refocus, sectionHeading } from '#lib/focus.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from './Icon.svelte';

	interface Props {
		value: Live<T>;
		/** Loaded, with nothing in it. */
		empty?: boolean;
		emptyText?: string;
		emptyHint?: string;
		/** Skeleton tiles while loading (their final height, so nothing jumps). */
		skeletons?: number;
		children: Snippet;
	}
	let { value, empty = false, emptyText, emptyHint, skeletons = 0, children }: Props = $props();
	const g = new Gesture();
	let box = $state<HTMLElement>();

	function retry() {
		return g.run(
			() => value.refresh(),
			// the retry button goes with the error: the focus goes to the group's title
			() => (value.failed ? undefined : refocus(sectionHeading(box))),
			'retry'
		);
	}
</script>

{#if value.loading}
	{#if skeletons}
		<div class="tiles" aria-hidden="true">
			{#each { length: skeletons }, i (i)}<div class="tile skeleton"></div>{/each}
		</div>
		<p class="sr-only">{m.common_loading()}</p>
	{:else}
		<p class="hint">{m.common_loading()}</p>
	{/if}
{:else if value.failed}
	<div class="empty" bind:this={box}>
		<p>{m.load_failed()}</p>
		<p class="hint">{errorText(value.error)}</p>
		<button class="btn" onclick={retry} {...pending(g.is('retry'))}><Icon name="refresh-cw" busy={g.is('retry')} />{m.common_retry()}</button>
	</div>
{:else if empty}
	<div class="empty">
		{#if emptyText}<p>{emptyText}</p>{/if}
		{#if emptyHint}<p class="hint">{emptyHint}</p>{/if}
	</div>
{:else}
	{@render children()}
{/if}
