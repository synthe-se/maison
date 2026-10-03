<script lang="ts">
	// The box's app shortcuts, as chips (its tile and its page): the app in front is marked
	// current; offline, they stay in place, unavailable, the tile's line saying why.
	import { m } from '#lib/paraglide/messages.js';
	import { pending, unavailable } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import type { Box } from './box.svelte.ts';

	let { box }: { box: Box } = $props();
	const id = $props.id();
</script>

<div class="apps" role="group" aria-label={m.android_tv_apps()}>
	{#each box.shortcuts as app (app.package)}
		{@const here = box.status?.currentApp === app.package}
		<button
			class="pill-btn"
			class:on={here}
			aria-current={here ? 'true' : undefined}
			{...box.reachable ? pending(box.apps.is()) : unavailable(`${id}-why`)}
			onclick={() => box.reachable && box.launch(app)}
		>
			{#if box.apps.is(app.package)}<Icon name="play" busy />{/if}{app.label}
		</button>
	{/each}
	{#if !box.reachable}<span class="sr-only" id="{id}-why">{m.android_tv_apps_unreachable()}</span>{/if}
</div>

<style>
	.apps {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-2);
	}
	.apps .pill-btn {
		min-height: var(--control-h);
	}
	.apps .pill-btn[aria-disabled='true']:not([aria-busy='true']) {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}
</style>
