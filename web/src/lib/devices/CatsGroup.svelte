<script lang="ts">
	// The cats' corner (Tuya, local): feeder, fountain, litter box. Grouped by place, not by
	// protocol (docs/ux.md § 5); the protocol stays in the detail. The group's ⋯ menu holds the
	// local connections (a labelled switch per device, as on each page), never a tile icon that
	// looks like a signal indicator.
	import { DropdownMenu } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import { devicesApi } from './cats/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
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

	const everyone = (run: () => Promise<unknown>, said: string) =>
		menu.run(run, async () => {
			ui.toast(said);
			await list.refresh();
		});
</script>

<Group id="cats-title" title={m.dashboard_cats_title()} fact={list.data ? m.dashboard_device_count({ count: list.data.total }) : undefined}>
	{#snippet actions()}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="icon-btn" aria-label={m.dashboard_cats_actions()}><Icon name="ellipsis" /></DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="menu" align="end" sideOffset={4}>
					{#if all.length}
						<DropdownMenu.Group>
							<DropdownMenu.GroupHeading class="menu-heading">{m.device_local_connection()}</DropdownMenu.GroupHeading>
							{#each all as d (d.id)}
								<DropdownMenu.CheckboxItem
									class="menu-item"
									checked={d.connected}
									closeOnSelect={false}
									onCheckedChange={(on) => setConnection(menu, d, on)}
								>
									{#snippet children({ checked })}
										<span class="box" aria-hidden="true"
											>{#if checked}<Icon name="check" size={14} />{/if}</span
										>{d.name}
									{/snippet}
								</DropdownMenu.CheckboxItem>
							{/each}
						</DropdownMenu.Group>
						<DropdownMenu.Separator class="menu-separator" />
					{/if}
					<DropdownMenu.Item class="menu-item" onSelect={() => list.refresh()}
						><Icon name="refresh-cw" />{m.common_refresh()}</DropdownMenu.Item
					>
					<DropdownMenu.Item
						class="menu-item"
						onSelect={() => everyone(devicesApi.connectAll, m.dashboard_global_connection_description())}
					>
						<Icon name="wifi" />{m.dashboard_connect_all()}
					</DropdownMenu.Item>
					<DropdownMenu.Item
						class="menu-item"
						onSelect={() => everyone(devicesApi.disconnectAll, m.dashboard_global_disconnection_description())}
					>
						<Icon name="wifi-off" />{m.dashboard_disconnect_all()}
					</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
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

<style>
	.box {
		display: inline-grid;
		place-items: center;
		width: var(--check-box);
		height: var(--check-box);
		flex: none;
		border: 1.5px solid var(--ink-muted);
		border-radius: var(--radius-s);
	}
</style>
