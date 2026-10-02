<script lang="ts">
	// Meross MSS310 plugs on the local network: one tile each, the count in the group head.
	// Only a plug that answers is asked for its power (as the old card did).
	import { m } from '#lib/paraglide/messages.js';
	import { merossApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import PlugTile from './meross/PlugTile.svelte';
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

	{#if list.loading}
		<p class="hint">{m.common_loading()}</p>
	{:else if plugs.length === 0}
		<div class="empty">
			<p>{m.meross_none()}</p>
			<p class="hint">{m.meross_none_hint()}</p>
		</div>
	{:else}
		<div class="tiles">
			{#each plugs as p (p.id)}
				{#if p.isOnline}<LivePlugTile plug={p} />{:else}<PlugTile id={p.id} name={p.name} on={p.isOn} online={false} />{/if}
			{/each}
		</div>
	{/if}
</section>
