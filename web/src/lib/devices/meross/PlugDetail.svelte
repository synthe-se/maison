<script lang="ts">
	import PageHead from '#lib/components/PageHead.svelte';
	// A plug's page: on/off with live power, electricity, daily consumption, the hardware,
	// and the indicator light. Same endpoints and cadences as the old React page.
	import { m } from '#lib/paraglide/messages.js';
	import { merossApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import Loaded from '#lib/components/Loaded.svelte';
	import { num, shortDay } from '#lib/i18n.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import PlugTile from './PlugTile.svelte';
	import { CONSUMPTION_EVERY_MS, ELECTRICITY_EVERY_MS, STATUS_EVERY_MS, consumptionKey, electricityKey, statusKey } from './keys.ts';
	import { kwh, kwhFromWh, milliamps, reading, volts, watts } from './units.ts';

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally (the route re-creates this view when the id changes)
	const plugId = id;
	const status = live(statusKey(plugId), () => merossApi.status(plugId), STATUS_EVERY_MS);
	const elec = live(electricityKey(plugId), () => merossApi.electricity(plugId), ELECTRICITY_EVERY_MS);
	const consumption = live(consumptionKey(plugId), () => merossApi.consumption(plugId), CONSUMPTION_EVERY_MS);

	const s = $derived(status.data?.status);
	const device = $derived(status.data?.device);
	const e = $derived(elec.data ? reading(elec.data.electricity.raw) : undefined);
	/** The last week, newest first. */
	const days = $derived((consumption.data?.consumption ?? []).slice(-7).reverse());

	const dnd = new Gesture();
	const setDnd = (enabled: boolean) => dnd.run(() => merossApi.dnd(plugId, enabled), () => ui.toast(m.meross_dnd_updated()), enabled ? 'off' : 'on');

	/** « mar. 30 sept. » from the plug's « 2026-09-30 »; anything else as it came. */
	function day(iso: string): string {
		return /^\d{4}-\d{2}-\d{2}$/.test(iso) ? shortDay(iso) : iso;
	}

	const info = $derived(
		s
			? [
					[m.device_model(), s.hardware?.type?.toUpperCase()],
					[m.meross_hw_version(), s.hardware?.version],
					[m.device_firmware(), s.firmware?.version],
					[m.meross_chip(), s.hardware?.chipType],
					[m.meross_mac(), s.hardware?.mac],
					[m.meross_ip(), s.firmware?.innerIp || device?.id],
					...(s.wifi.signal !== null ? [[m.meross_wifi_signal(), m.meross_signal_percent({ signal: num(s.wifi.signal) })]] : [])
				]
			: []
	);
</script>

<PageHead back title={device?.name ?? m.meross_plug_control()} />

<div class="measure">
<Loaded value={status} empty={!s || !device} emptyText={m.meross_not_found()}>
{#if s && device}
	<div class="detail measure">
		{#if !s.online}
			<div class="callout warn">
				<p><strong>{m.meross_not_connected()}</strong></p>
				<p>{m.meross_not_connected_description()}</p>
			</div>
		{/if}

		<PlugTile id={plugId} name={device.name} on={s.on} online={s.online} fact={e ? watts(e.watts, 1) : undefined} link={false} />

		<section class="group" aria-labelledby="elec-title">
			<h2 id="elec-title" class="group-title">{m.meross_electricity()}</h2>
			<dl class="facts">
				<div><dt>{m.meross_voltage()}</dt><dd>{e ? volts(e.volts) : '—'}</dd></div>
				<div><dt>{m.meross_current()}</dt><dd>{e ? milliamps(e.amps) : '—'}</dd></div>
				<div><dt>{m.meross_power()}</dt><dd>{e ? watts(e.watts, 1) : '—'}</dd></div>
			</dl>
		</section>

		{#if consumption.data && days.length}
			<section class="group" aria-labelledby="conso-title">
				<div class="group-head">
					<h2 id="conso-title" class="group-title">{m.meross_consumption()}</h2>
					<span class="fact">{m.meross_consumption_days({ count: consumption.data.summary.days })} · {kwh(consumption.data.summary.totalKwh)}</span>
				</div>
				<ul class="list">
					{#each days as d (d.date)}
						<li class="row"><span>{day(d.date)}</span><span class="num">{kwhFromWh(d.value)}</span></li>
					{/each}
				</ul>
			</section>
		{/if}

		<section class="group" aria-labelledby="info-title">
			<h2 id="info-title" class="group-title">{m.meross_device_info()}</h2>
			<dl class="info">
				{#each info as [label, value] (label)}
					<div><dt>{label}</dt><dd>{value || m.common_unknown()}</dd></div>
				{/each}
			</dl>
		</section>

		<!-- the plug never reports its LED setting: two orders, no pressed state (§ 2, case 3) -->
		<section class="group" aria-labelledby="dnd-title">
			<h2 id="dnd-title" class="group-title">{m.meross_dnd_mode()}</h2>
			<p class="hint">{m.meross_dnd_description()}</p>
			<div class="btn-row">
				<button class="btn" {...pending(dnd.is('off'))} onclick={() => setDnd(true)}>
					<Icon name="bell-off" busy={dnd.is('off')} /><span class="btn-text">{m.meross_led_off()}</span>
				</button>
				<button class="btn" {...pending(dnd.is('on'))} onclick={() => setDnd(false)}>
					<Icon name="sun" busy={dnd.is('on')} /><span class="btn-text">{m.meross_led_on()}</span>
				</button>
			</div>
		</section>
	</div>
{/if}
</Loaded>
</div>

<style>
	/* one column that may shrink to the screen (a long word inside never widens the page) */
	.detail { display: grid; grid-template-columns: minmax(0, 1fr); gap: var(--s-6); }
	.detail .group + .group { margin-top: 0; }
	dl { margin: 0; }
	dd { margin: 0; font-variant-numeric: tabular-nums; }
	dt { font: var(--t-secondary); color: var(--ink-muted); }
	/* three readings side by side while they fit, one under the other on a narrow phone (1.4.10) */
	.facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(6rem, 100%), 1fr)); gap: var(--s-3); }
	.facts dd { font: var(--t-group); }
	.info { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(12rem, 100%), 1fr)); gap: var(--s-3); }
	.info dd { overflow-wrap: anywhere; }
	.row { display: flex; justify-content: space-between; gap: var(--s-3); padding: var(--s-2) 0; border-bottom: 1px solid var(--line); }
	.num { font-variant-numeric: tabular-nums; }
</style>
