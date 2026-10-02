<script lang="ts">
	import { clock } from '#lib/i18n.svelte.ts';
	// The cats' corner (Tuya, local): feeder, fountain, litter box. Grouped by place, not by
	// protocol (docs/ux/tableau-de-bord.md § 5); the protocol stays in the detail.
	import { DropdownMenu } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import { devicesApi, type Device } from '#lib/api.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import ConnectButton from './cats/ConnectButton.svelte';
	import { ICON, devices, feed, served } from './cats/tuya.svelte.ts';

	/** Skeleton tiles while the first answer comes (same height as a tile, § 4). */
	const SKELETONS = 3;

	const list = devices();
	const all = $derived(list.data?.devices ?? []);
	// two gestures: connecting everyone from the menu may travel while a feeder serves, and
	// must not clear the serving mark on its tile
	const feeding = new Gesture();
	const menu = new Gesture();

	const everyone = (run: () => Promise<unknown>, said: string) =>
		menu.run(run, async () => {
			ui.toast(said);
			await list.refresh();
		});

	const give = (d: Device) => feed(feeding, d.id, 1, d.id);

	const describe = (d: Device) =>
		!d.connected ? m.device_offline() : served[d.id] ? m.feeder_served_at({ time: clock(served[d.id]) }) : m.device_online();
</script>

<section class="group" aria-labelledby="cats-title">
	<div class="group-head">
		<h2 id="cats-title" class="group-title">{m.dashboard_cats_title()}</h2>
		{#if list.data}<span class="fact">{m.dashboard_device_count({ count: list.data.total })}</span>{/if}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger class="icon-btn" aria-label={m.dashboard_cats_actions()}><Icon name="settings-2" /></DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content class="menu" align="end" sideOffset={4}>
					<DropdownMenu.Item class="menu-item" onSelect={() => list.refresh()}><Icon name="refresh-cw" />{m.common_refresh()}</DropdownMenu.Item>
					<DropdownMenu.Item class="menu-item" onSelect={() => everyone(devicesApi.connectAll, m.dashboard_global_connection_description())}>
						<Icon name="wifi" />{m.dashboard_connect_all()}
					</DropdownMenu.Item>
					<DropdownMenu.Item class="menu-item" onSelect={() => everyone(devicesApi.disconnectAll, m.dashboard_global_disconnection_description())}>
						<Icon name="wifi-off" />{m.dashboard_disconnect_all()}
					</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	</div>

	{#if list.loading}
		<p class="sr-only" role="status">{m.common_loading()}</p>
		<div class="tiles" aria-hidden="true">
			{#each { length: SKELETONS }, i (i)}<div class="tile skeleton"></div>{/each}
		</div>
	{:else if list.error && !list.data}
		<div class="empty">
			<p>{m.dashboard_loading_error()}</p>
			<button class="btn" onclick={() => list.refresh()}><Icon name="refresh-cw" />{m.common_retry()}</button>
		</div>
	{:else if all.length === 0}
		<div class="empty">
			<p>{m.dashboard_no_devices()}</p>
			<p class="hint">{m.dashboard_no_devices_hint()}</p>
		</div>
	{:else}
		<div class="tiles">
			{#each all as d (d.id)}
				<DeviceTile name={d.name} icon={ICON[d.type]} href="/device/{d.id}" state={describe(d)} warn={!d.connected} on={d.connected}>
					{#snippet end()}<ConnectButton device={d} />{/snippet}
					{#if d.type === 'feeder'}
						<button class="btn give" disabled={!d.connected || feeding.is(d.id)} aria-busy={feeding.is(d.id)} onclick={() => give(d)}>
							<Icon name={feeding.is(d.id) ? 'loader-circle' : 'utensils'} class={feeding.is(d.id) ? 'spin' : undefined} />{m.feeder_distribute({ count: 1 })}
						</button>
					{/if}
				</DeviceTile>
			{/each}
		</div>
	{/if}
</section>

<style>
	.give { min-height: var(--control-h); justify-content: center; }
	/* a tile's final height: head row plus padding (no jump when the list arrives) */
	.skeleton { min-height: calc(var(--tile-icon) + 2 * var(--s-4)); background: var(--ground-raised); border-color: transparent; }
	.empty .btn { margin-top: var(--s-3); }
</style>
