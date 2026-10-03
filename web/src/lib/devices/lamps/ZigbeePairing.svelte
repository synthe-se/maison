<script lang="ts">
	// Opening the Zigbee network to new lamps (permit join) and the Touchlink scan for
	// factory-new ones. The window's countdown is read every second while it is open. Start and
	// Stop are one button whose words change (a plain button, not a pressed toggle): the focus
	// stays on it from one to the other.
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi } from './api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import { zigbee, zigbeePairing } from './lamp.ts';

	const status = live(zigbeePairing);
	const pairing = $derived(status.data?.pairing);
	const open = $derived(pairing?.active ?? false);

	const g = new Gesture();

	function toggle() {
		return g.run(
			() => (open ? zigbeeLampsApi.stopPairing() : zigbeeLampsApi.startPairing()),
			(r) => {
				// the countdown starts at once (Live.set plans the next read)
				status.set(r);
				// the countdown is on screen; a reader hears the outcome once
				ui.say(r.pairing.active ? m.zigbee_lamps_pairing_active({ count: r.pairing.remainingSeconds }) : m.zigbee_lamps_pairing_inactive());
				void refresh(zigbee.key);
			},
			'window'
		);
	}

	const touchlink = () =>
		g.run(
			() => zigbeeLampsApi.touchlinkScan(),
			() => ui.toast(m.zigbee_lamps_touchlink_started()),
			'touchlink'
		);
</script>

<div class="inset measure">
	<p class="state">
		{open ? m.zigbee_lamps_pairing_active({ count: pairing!.remainingSeconds }) : m.zigbee_lamps_pairing_inactive()}
	</p>
	{#if pairing?.message}<p class="hint">{pairing.message}</p>{/if}
	<div class="actions">
		<button class="btn primary" {...pending(g.is('window'))} onclick={toggle}>
			<Icon name={open ? 'square' : 'radio'} busy={g.is('window')} />{open ? m.zigbee_lamps_stop_pairing() : m.zigbee_lamps_start_pairing()}
		</button>
		<button class="btn" {...pending(g.is('touchlink'))} onclick={touchlink}>
			<Icon name="search" busy={g.is('touchlink')} />{m.zigbee_lamps_touchlink_scan()}
		</button>
	</div>
</div>

<style>
	.state {
		margin: 0;
		font: var(--t-label);
		font-variant-numeric: tabular-nums;
	}
</style>
