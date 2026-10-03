<script lang="ts">
	// A plug's page: on/off with live power and what it costs now, electricity, daily
	// consumption and the month's estimated cost (each day at its Tempo color), the hardware,
	// and the indicator light.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { isoDay, num, percent, shortDay } from '#lib/i18n.svelte.ts';
	import { time } from '#lib/clock.svelte.ts';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import StatusRow from '#lib/components/StatusRow.svelte';
	import { calendar, today } from '#lib/devices/tempo/data.ts';
	import { seasonOf } from '#lib/devices/tempo/dates.ts';
	import { euros, monthEstimate, peakShare, priceNow } from '#lib/devices/tempo/price.ts';
	import { merossApi } from './api.ts';
	import { consumption as consumptionOf, electricity, status as statusOf } from './data.ts';
	import PlugTile from './PlugTile.svelte';
	import { kwh, kwhFromWh, milliamps, powerAndCost, reading, volts, watts } from './units.ts';

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally (the route re-creates this view when the id changes)
	const plugId = id;
	const status = live(statusOf(plugId));
	const elec = live(electricity(plugId));
	const consumption = live(consumptionOf(plugId));

	const s = $derived(status.data?.status);
	const device = $derived(status.data?.device);
	const e = $derived(elec.data ? reading(elec.data.electricity.raw) : undefined);
	/** The last week, newest first. */
	const days = $derived((consumption.data?.consumption ?? []).slice(-7).toReversed());

	// what electricity costs: the price in force, and each day's color this season
	const tempo = live(today);
	// svelte-ignore state_referenced_locally (a page is opened at one moment: its season)
	const season = live(calendar(seasonOf(time.now.getFullYear(), time.now.getMonth())));
	const price = $derived(tempo.data ? priceNow(time.now, tempo.data) : null);
	/** « 2026-10 » */
	const month = $derived(isoDay(time.now).slice(0, 7));
	const colorOf = (iso: string) =>
		season.data?.calendar.find((d) => d.date === iso && d.isActual)?.color ??
		[tempo.data?.today, tempo.data?.yesterday].find((d) => d?.date === iso)?.color;
	const estimate = $derived(
		tempo.data?.tariffs && consumption.data
			? monthEstimate(
					consumption.data.consumption.map((d) => ({ date: d.date, wh: d.value })),
					colorOf,
					tempo.data.tariffs,
					tempo.data.hours,
					month
				)
			: null
	);

	const dnd = new Gesture();
	const setDnd = (enabled: boolean) =>
		dnd.run(
			() => merossApi.dnd(plugId, enabled),
			() => ui.toast(m.meross_dnd_updated()),
			enabled ? 'off' : 'on'
		);

	/** « mar. 30 sept. » from the plug's « 2026-09-30 »; anything else as it came. */
	const day = (iso: string) => (/^\d{4}-\d{2}-\d{2}$/.test(iso) ? shortDay(iso) : iso);

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
	<Loaded value={status} missing={m.meross_not_found()} empty={!s || !device} emptyText={m.meross_not_found()}>
		{#if s && device}
			<div class="detail">
				{#if !s.online}
					<div class="callout warn">
						<p><strong>{m.meross_not_connected()}</strong></p>
						<p>{m.meross_not_connected_description()}</p>
					</div>
				{/if}

				<PlugTile
					id={plugId}
					name={device.name}
					on={s.on}
					online={s.online}
					fact={e ? powerAndCost(e.watts, s.on ? price?.price : undefined, 1) : undefined}
					link={false}
				/>

				<Group id="elec-title" title={m.meross_electricity()}>
					<dl class="readings">
						<div>
							<dt>{m.meross_voltage()}</dt>
							<dd>{e ? volts(e.volts) : '—'}</dd>
						</div>
						<div>
							<dt>{m.meross_current()}</dt>
							<dd>{e ? milliamps(e.amps) : '—'}</dd>
						</div>
						<div>
							<dt>{m.meross_power()}</dt>
							<dd>{e ? watts(e.watts, 1) : '—'}</dd>
						</div>
					</dl>
				</Group>

				{#if consumption.data && days.length}
					<Group
						id="conso-title"
						title={m.meross_consumption()}
						fact={`${m.meross_consumption_days({ count: consumption.data.summary.days })} · ${kwh(consumption.data.summary.totalKwh)}`}
					>
						{#if estimate && estimate.days > 0 && tempo.data}
							<div class="estimate">
								<p class="amount">
									<span>{m.meross_month_estimate({ cost: euros(estimate.euros) })}</span><span class="chip">{m.meross_estimate()}</span>
								</p>
								<p class="hint">{m.meross_estimate_hint({ count: estimate.days, peak: percent(peakShare(tempo.data.hours)) })}</p>
							</div>
						{/if}
						<dl class="facts">
							{#each days as d (d.date)}<StatusRow label={day(d.date)} value={kwhFromWh(d.value)} />{/each}
						</dl>
					</Group>
				{/if}

				<Group id="info-title" title={m.meross_device_info()}>
					<dl class="facts">
						{#each info as [label, value] (label)}<StatusRow label={label ?? ''} value={value || m.common_unknown()} />{/each}
					</dl>
				</Group>

				<!-- the plug never reports its LED setting: two orders, no pressed state (§ 2, case 3) -->
				<Group id="dnd-title" title={m.meross_dnd_mode()}>
					<p class="hint">{m.meross_dnd_description()}</p>
					<div class="btn-row">
						<button class="btn" {...pending(dnd.is('off'))} onclick={() => setDnd(true)}>
							<Icon name="bell-off" busy={dnd.is('off')} /><span class="btn-text">{m.meross_led_off()}</span>
						</button>
						<button class="btn" {...pending(dnd.is('on'))} onclick={() => setDnd(false)}>
							<Icon name="sun" busy={dnd.is('on')} /><span class="btn-text">{m.meross_led_on()}</span>
						</button>
					</div>
				</Group>
			</div>
		{/if}
	</Loaded>
</div>

<style>
	/* one column that may shrink to the screen (a long word inside never widens the page) */
	.detail {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: var(--s-6);
	}
	.detail :global(.group + .group) {
		margin-top: 0;
	}
	/* three readings side by side while they fit, one under the other on a narrow phone (1.4.10) */
	.readings {
		margin: 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(6rem, 100%), 1fr));
		gap: var(--s-3);
	}
	.readings dt {
		font: var(--t-secondary);
		color: var(--ink-muted);
	}
	.readings dd {
		margin: 0;
		font: var(--t-group);
		font-variant-numeric: tabular-nums;
	}
	.estimate {
		display: grid;
		gap: var(--s-1);
	}
	.amount {
		margin: 0;
		display: flex;
		align-items: center;
		gap: var(--s-2);
		font: var(--t-group);
		font-variant-numeric: tabular-nums;
	}
</style>
