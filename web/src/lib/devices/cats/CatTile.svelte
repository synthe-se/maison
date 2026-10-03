<script lang="ts">
	// One cat device on the dashboard (docs/ux.md § 1): its name leads to its page, line 2 says
	// what matters (« Eau basse », « Litière à moitié pleine », « Propre »; what needs a hand in
	// the warning style), « En ligne » only when there is nothing to say (alerts.ts). The feeder
	// gives one portion from here; offline, that button stays, unavailable, the line saying why.
	import { m } from '#lib/paraglide/messages.js';
	import type { Device } from './api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import { ICON, feed, served } from './tuya.svelte.ts';
	import { status } from './data.ts';
	import { tileLine } from './alerts.ts';

	let { device }: { device: Device } = $props();
	// svelte-ignore state_referenced_locally (one tile per device: the parent keys the list)
	const st = device.type === 'unknown' ? undefined : live(status(device.type, device.id));
	const line = $derived(tileLine(device, st?.data, served[device.id]));
	const g = new Gesture();
</script>

<DeviceTile name={device.name} icon={ICON[device.type]} href="/device/{device.id}" state={line.text} warn={line.warn} on={device.connected}>
	{#snippet end(why)}
		{#if device.type === 'feeder'}
			<button
				class="btn give"
				aria-label={m.feeder_distribute({ count: 1 })}
				{...device.connected ? pending(g.is()) : unavailable(why)}
				onclick={() => device.connected && feed(g, device.id, 1)}
			>
				<Icon name="utensils" busy={g.is()} />{m.feeder_one_portion()}
			</button>
		{/if}
	{/snippet}
</DeviceTile>

<style>
	.give {
		white-space: nowrap;
	}
</style>
