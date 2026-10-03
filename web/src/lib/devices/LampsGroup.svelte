<script lang="ts">
	// Every lamp in one group, whatever its radio (docs/ux.md § 5: by type, not by protocol):
	// Hue over Bluetooth and Zigbee lamps on the server's coordinator, each family left out when
	// the server runs without it. The radio is said on the lamp's page. « Tout éteindre » when
	// one is on; adding a lamp (a Bluetooth search, or opening the Zigbee network) is an admin's.
	import { DropdownMenu } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import { hueLampsApi } from './lamps/api.ts';
	import { live, refresh, type Loadable } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Group from '#lib/components/Group.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import LampTile from './lamps/LampTile.svelte';
	import ZigbeePairing from './lamps/ZigbeePairing.svelte';
	import { fromHue, fromZigbee, hue, zigbee, type Lamp, type LampDriver } from './lamps/lamp.ts';
	import { everyone, sayOutcome } from './everyone.ts';

	const hueStats = live(hue.stats);
	const zigbeeStats = live(zigbee.stats);
	const hueList = live(hue.list);
	const zigbeeList = live(zigbee.list);

	const families = $derived(
		[
			{ on: hueStats.data?.disabled !== true, list: hueList, driver: hue, lamps: (hueList.data?.lamps ?? []).map(fromHue) },
			{ on: zigbeeStats.data?.disabled !== true, list: zigbeeList, driver: zigbee, lamps: (zigbeeList.data?.lamps ?? []).map(fromZigbee) }
		].filter((f) => f.on)
	);
	type Row = { lamp: Lamp; driver: LampDriver };
	const rows: Row[] = $derived(families.flatMap((f) => f.lamps.map((lamp) => ({ lamp, driver: f.driver }))));
	const lit = $derived(rows.filter((r) => r.lamp.reachable && r.lamp.isOn));

	/** The two lists read as one: loading while one is, failed when one did and nothing is
	 * known (never « no lamp » because a radio did not answer), old when one is (or one failed
	 * while the other shows its lamps), read when the older one was. */
	const lists: Loadable = {
		get loading() {
			return families.some((f) => f.list.loading);
		},
		get failed() {
			return families.some((f) => f.list.failed) && rows.length === 0;
		},
		get stale() {
			return rows.length > 0 && families.some((f) => f.list.stale || f.list.failed);
		},
		get at() {
			return Math.min(...families.map((f) => f.list.at));
		},
		get error() {
			return families.find((f) => f.list.error)?.list.error;
		},
		refresh: () => Promise.all(families.map((f) => f.list.refresh()))
	};

	const g = new Gesture();
	let pairing = $state(false);
	let title = $state<HTMLElement>();
	let offButton = $state<HTMLButtonElement>();

	const allOff = () =>
		g.run(
			() =>
				everyone(
					lit,
					(r) => r.lamp.name,
					(r) => r.driver.power(r.lamp.id, false)
				),
			async (o) => {
				sayOutcome(o, (count) => m.lamps_all_off_done({ count }));
				await Promise.all([refresh(hue.key), refresh(zigbee.key)]);
				// none lit any more: the button is gone, the focus goes to the group's title
				await refocus(offButton, title);
			},
			'off'
		);

	const scan = () =>
		g.run(
			() => hueLampsApi.scan(),
			async () => {
				ui.toast(m.hue_lamps_scan_started());
				await refresh(hue.key);
			},
			'scan'
		);
	const zigbeeOn = $derived(families.some((f) => f.driver === zigbee));
	const hueOn = $derived(families.some((f) => f.driver === hue));
</script>

{#if families.length}
	<Group id="lamps-title" title={m.zigbee_lamps_title()} bind:heading={title}>
		{#snippet actions()}
			{#if lit.length}
				<button class="btn" bind:this={offButton} {...pending(g.is('off'))} onclick={allOff}>
					<Icon name="lightbulb-off" busy={g.is('off')} />{m.lamps_all_off()}
				</button>
			{/if}
			<AdminOnly reason={false}>
				<DropdownMenu.Root>
					<DropdownMenu.Trigger class="icon-btn" aria-label={m.lamps_add()} {...pending(g.is('scan'))}>
						<Icon name="plus" busy={g.is('scan')} />
					</DropdownMenu.Trigger>
					<DropdownMenu.Portal>
						<DropdownMenu.Content class="menu" align="end" sideOffset={4}>
							{#if hueOn}
								<DropdownMenu.Item class="menu-item" onSelect={scan}><Icon name="search" />{m.lamps_add_bluetooth()}</DropdownMenu.Item>
							{/if}
							{#if zigbeeOn}
								<DropdownMenu.Item class="menu-item" onSelect={() => (pairing = !pairing)}>
									<Icon name="radio" />{pairing ? m.lamps_add_zigbee_close() : m.lamps_add_zigbee()}
								</DropdownMenu.Item>
							{/if}
						</DropdownMenu.Content>
					</DropdownMenu.Portal>
				</DropdownMenu.Root>
			</AdminOnly>
		{/snippet}

		{#if pairing && zigbeeOn}<ZigbeePairing />{/if}

		<Loaded value={lists} empty={rows.length === 0} emptyText={m.lamps_none()} emptyHint={m.lamps_none_hint()}>
			<div class="tiles">
				{#each rows as r (`${r.driver.key}:${r.lamp.id}`)}
					<LampTile lamp={r.lamp} driver={r.driver} />
				{/each}
			</div>
		</Loaded>
	</Group>
{/if}
