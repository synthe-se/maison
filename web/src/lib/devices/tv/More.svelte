<script lang="ts">
	// The rest of a remote, folded under its tile: the pad and the rarer keys. Open from 600 px
	// (room to spare), folded on a phone so the dashboard stays a list of tiles; there is no TV
	// page (yet) to send them to.
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';

	let { children }: { children: Snippet } = $props();
	const id = $props.id();
	let open = $state(matchMedia('(min-width: 600px)').matches);
</script>

<button class="btn ghost more" aria-expanded={open} aria-controls="{id}-more" onclick={() => (open = !open)}>
	<Icon name={open ? 'chevron-up' : 'chevron-down'} />{m.tv_more_controls()}
</button>
<div id="{id}-more" class="body" hidden={!open}>
	{@render children()}
</div>

<style>
	.more { justify-self: start; min-height: var(--control-h); }
	.body { display: grid; gap: var(--s-4); }
</style>
