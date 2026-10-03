<script lang="ts">
	// Roller shutters over Matter. The list polls slowly at rest and every second while a motor
	// runs, so the position follows the travel (data.ts). Positions are « how open » (100 =
	// open). Adding (commissioning, AddShutter) and removing a shutter are an admin's; moving,
	// renaming and the sun schedule are everyone's (ShutterTile). « Tout ouvrir » / « Tout
	// fermer » move every shutter at once.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { shuttersApi } from './shutters/api.ts';
	import { covers as coverList } from './shutters/data.ts';
	import AddShutter from './shutters/AddShutter.svelte';
	import ShutterTile from './shutters/ShutterTile.svelte';
	import { everyone, sayOutcome } from './everyone.ts';

	const list = live(coverList);
	const covers = $derived(list.data?.covers ?? []);
	const online = $derived(covers.filter((c) => c.online));
	let adding = $state(false);
	let title = $state<HTMLElement>();

	const all = new Gesture();
	function moveAll(open: boolean) {
		haptic(CONFIRM);
		return all.run(
			() =>
				everyone(
					online,
					(c) => c.name,
					(c) => (open ? shuttersApi.open(c.id) : shuttersApi.close(c.id))
				),
			async (o) => {
				sayOutcome(o, (count) => (open ? m.shutters_all_opening({ count }) : m.shutters_all_closing({ count })));
				await list.refresh();
			},
			open ? 'open' : 'close'
		);
	}
</script>

<Group id="shutters-title" title={m.shutters_title()} bind:heading={title}>
	{#snippet actions()}
		{#if online.length}
			<button class="btn" {...pending(all.is('open'))} onclick={() => moveAll(true)}
				><Icon name="arrow-up" busy={all.is('open')} />{m.shutters_all_open()}</button
			>
			<button class="btn" {...pending(all.is('close'))} onclick={() => moveAll(false)}
				><Icon name="arrow-down" busy={all.is('close')} />{m.shutters_all_close()}</button
			>
		{/if}
		<AdminOnly reason={false}>
			<button class="icon-btn" aria-label={m.shutters_add()} aria-haspopup="dialog" onclick={() => (adding = true)}
				><Icon name="plus" /></button
			>
		</AdminOnly>
	{/snippet}

	<Loaded value={list} empty={covers.length === 0} emptyText={m.shutters_none()} emptyHint={m.shutters_none_hint()}>
		<div class="tiles">
			<!-- its tile gone with it, a removed shutter gives the focus to the group's title -->
			{#each covers as c (c.id)}<ShutterTile cover={c} {list} onremoved={() => refocus(title)} />{/each}
		</div>
	</Loaded>
</Group>

<AddShutter bind:open={adding} onadded={() => list.refresh()} />
