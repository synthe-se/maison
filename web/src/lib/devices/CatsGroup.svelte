<script lang="ts">
	// The cats' corner (Tuya, local): feeder, fountain, litter box. Grouped by place, not by
	// protocol (docs/ux.md § 5); the protocol stays in the detail. The group's ⋯ menu holds the
	// local connections (a labelled switch per device, as on each page), never a tile icon that
	// looks like a signal indicator.
	import { m } from '#lib/paraglide/messages.js';
	import { devicesApi } from './cats/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Menu from '#lib/components/Menu.svelte';
	import Group from '#lib/components/Group.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import CatTile from './cats/CatTile.svelte';
	import { devices } from './cats/data.ts';
	import { setConnection } from './cats/connection.ts';

	/** Skeleton tiles while the first answer comes (same height as a tile, § 4). */
	const SKELETONS = 3;

	const list = live(devices);
	const all = $derived(list.data?.devices ?? []);
	const menu = new Gesture();

	const forAll = (run: () => Promise<unknown>, said: string) =>
		menu.run(run, async () => {
			ui.toast(said);
			await list.refresh();
		});
</script>

<Group id="cats-title" title={m.dashboard_cats_title()} fact={list.data ? m.dashboard_device_count({ count: list.data.total }) : undefined}>
	{#snippet actions()}
		<Menu
			label={m.dashboard_cats_actions()}
			busy={menu.is()}
			sections={[
				{
					heading: m.device_local_connection(),
					items: all.map((d) => ({ label: d.name, checked: d.connected, oncheck: (on: boolean) => setConnection(menu, d, on) }))
				},
				{
					items: [
						{ label: m.common_refresh(), icon: 'refresh-cw', onselect: () => list.refresh() },
						{
							label: m.dashboard_connect_all(),
							icon: 'wifi',
							onselect: () => forAll(devicesApi.connectAll, m.dashboard_global_connection_description())
						},
						{
							label: m.dashboard_disconnect_all(),
							icon: 'wifi-off',
							onselect: () => forAll(devicesApi.disconnectAll, m.dashboard_global_disconnection_description())
						}
					]
				}
			]}
		/>
	{/snippet}

	<Loaded
		value={list}
		skeletons={SKELETONS}
		empty={all.length === 0}
		emptyText={m.dashboard_no_devices()}
		emptyHint={m.dashboard_no_devices_hint()}
	>
		<div class="tiles">
			{#each all as d (d.id)}<CatTile device={d} />{/each}
		</div>
	</Loaded>
</Group>
