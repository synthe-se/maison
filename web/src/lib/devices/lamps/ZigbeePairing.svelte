<script lang="ts">
	// Opening the Zigbee network to new lamps (permit join) and the Touchlink scan for
	// factory-new ones. The window's countdown is read every second while it is open.
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi } from '#lib/api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import { zigbee } from './lamp.ts';

	/** While open: every second, for the countdown (as React did); closed: slower. */
	const OPEN_EVERY = 1_000;
	const CLOSED_EVERY = 10_000;

	const status = live('zigbee-pairing', zigbeeLampsApi.pairingStatus, (d) => (d?.pairing.active ? OPEN_EVERY : CLOSED_EVERY));
	const pairing = $derived(status.data?.pairing);

	const g = new Gesture();

	function act(which: 'start' | 'stop' | 'touchlink') {
		if (which === 'touchlink') return g.run(() => zigbeeLampsApi.touchlinkScan(), () => ui.toast(m.zigbee_lamps_touchlink_started()), which);
		return g.run(
			() => (which === 'start' ? zigbeeLampsApi.startPairing() : zigbeeLampsApi.stopPairing()),
			(r) => {
				status.set(r);
				// the countdown is on screen; a reader hears the outcome once (live regions via ui only)
				ui.say(r.pairing.active ? m.zigbee_lamps_pairing_active({ count: r.pairing.remainingSeconds }) : m.zigbee_lamps_pairing_inactive());
				void refresh(zigbee.key);
			},
			which
		);
	}
</script>

<div class="inset measure">
	<p class="state">
		{pairing?.active ? m.zigbee_lamps_pairing_active({ count: pairing.remainingSeconds }) : m.zigbee_lamps_pairing_inactive()}
	</p>
	{#if pairing?.message}<p class="hint">{pairing.message}</p>{/if}
	<div class="actions">
		{#if pairing?.active}
			<button class="btn primary" disabled={g.is('stop')} onclick={() => act('stop')}>
				{#if g.is('stop')}<Icon name="loader-circle" class="spin" />{/if}{m.zigbee_lamps_stop_pairing()}
			</button>
		{:else}
			<button class="btn primary" disabled={g.is('start')} onclick={() => act('start')}>
				{#if g.is('start')}<Icon name="loader-circle" class="spin" />{/if}{m.zigbee_lamps_start_pairing()}
			</button>
		{/if}
		<button class="btn" disabled={g.is('touchlink')} onclick={() => act('touchlink')}>
			{#if g.is('touchlink')}<Icon name="loader-circle" class="spin" />{/if}{m.zigbee_lamps_touchlink_scan()}
		</button>
	</div>
</div>

<style>
	.state { margin: 0; font: var(--t-label); font-variant-numeric: tabular-nums; }
</style>
