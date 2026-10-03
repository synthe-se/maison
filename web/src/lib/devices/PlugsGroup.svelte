<script lang="ts">
	// Meross MSS310 plugs on the local network: one tile each, the count in the group head.
	// Only a plug that answers is asked for its power (as the old card did).
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import Group from '#lib/components/Group.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import LivePlugTile from './meross/LivePlugTile.svelte';
	import { plugs as plugList } from './meross/data.ts';

	const list = live(plugList);
	const plugs = $derived(list.data?.devices ?? []);
	const online = $derived(plugs.filter((p) => p.isOnline).length);
</script>

<Group
	id="meross-title"
	title={m.meross_title()}
	fact={plugs.length
		? `${m.meross_plug_count({ count: list.data?.total ?? plugs.length })} · ${m.meross_online_count({ count: online })}`
		: undefined}
>
	<Loaded value={list} empty={plugs.length === 0} emptyText={m.meross_none()} emptyHint={m.meross_none_hint()}>
		<div class="tiles">
			<!-- one component whether it answers or not: a plug going offline keeps its tile -->
			{#each plugs as p (p.id)}<LivePlugTile plug={p} />{/each}
		</div>
	</Loaded>
</Group>
