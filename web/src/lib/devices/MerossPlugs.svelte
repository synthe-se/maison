<script lang="ts">
	// Meross MSS310 plugs on the local network: one tile each, the count in the group head.
	// Only a plug that answers is asked for its power (as the old card did).
	import { m } from '#lib/paraglide/messages.js';
	import { merossApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import Loaded from '#lib/components/Loaded.svelte';
	import LivePlugTile from './meross/LivePlugTile.svelte';
	import { LIST_EVERY_MS, LIST_KEY } from './meross/keys.ts';

	const list = live(LIST_KEY, merossApi.list, LIST_EVERY_MS);
	const plugs = $derived(list.data?.devices ?? []);
	const online = $derived(plugs.filter((p) => p.isOnline).length);
</script>

<section class="group" aria-labelledby="meross-title">
	<div class="group-head">
		<h2 id="meross-title" class="group-title">{m.meross_title()}</h2>
		{#if plugs.length}
			<span class="fact">{m.meross_plug_count({ count: list.data?.total ?? plugs.length })} · {m.meross_online_count({ count: online })}</span>
		{/if}
	</div>

	<Loaded value={list} empty={plugs.length === 0} emptyText={m.meross_none()} emptyHint={m.meross_none_hint()}>
		<div class="tiles">
			<!-- one component whether it answers or not: a plug going offline keeps its tile -->
			{#each plugs as p (p.id)}<LivePlugTile plug={p} />{/each}
		</div>
	</Loaded>
</section>
