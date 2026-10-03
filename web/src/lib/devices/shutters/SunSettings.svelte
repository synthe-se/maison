<script lang="ts">
	// « Suivre le soleil » for one shutter: open at sunrise, close at sunset, each with its
	// offset, saved as soon as changed (settings rows, docs/ux.md § 2). The
	// house's place comes first: without it there is no sun to follow.
	import { m } from '#lib/paraglide/messages.js';
	import { shuttersApi, type Shutter, type SunSchedule } from './api.ts';
	import { place as placeData } from './data.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import SettingRow from '#lib/components/SettingRow.svelte';
	import Select from '#lib/components/Select.svelte';
	import PlacePicker from './PlacePicker.svelte';
	import { options } from '#lib/options.ts';
	import { dayAndTime, OFFSET_VALUES, offsetLabel, toggled } from './sun.ts';

	let { cover, onchange }: { cover: Shutter; onchange: (cover: Shutter) => void } = $props();

	const place = live(placeData);
	const g = new Gesture();
	const s = $derived(cover.schedule);
	const known = $derived(!!place.data?.place);

	// keyed by the control that asked: busy, it keeps the focus and ignores presses
	// (docs/ux.md § 2); the others stay free
	function save(next: SunSchedule, key: string) {
		return g.run(
			() => shuttersApi.setSchedule(cover.id, next),
			(r) => onchange(r.cover),
			key
		);
	}
</script>

<section class="sun" aria-labelledby="sun-{cover.id}">
	<h4 id="sun-{cover.id}" class="label">{m.shutters_sun_title()}</h4>
	<PlacePicker
		place={place.data?.place ?? null}
		onpicked={(p) => {
			place.update((d) => ({ ...d, place: p }));
			void place.refresh();
		}}
	/>
	{#if known}
		<ul class="settings">
			<SettingRow
				icon="sun"
				label={m.shutters_open_at_sunrise()}
				checked={s.openAtSunrise}
				busy={g.is('sunrise')}
				onchange={(on) => save(toggled(s, 'sunrise', on), 'sunrise')}
				hint={s.openAtSunrise && cover.nextOpen ? m.shutters_next_open({ when: dayAndTime(cover.nextOpen) }) : undefined}
			/>
			{#if s.openAtSunrise}
				<li class="offset">
					<Select
						label={m.shutters_offset()}
						value={String(s.sunriseOffsetMin)}
						options={options(OFFSET_VALUES, (v) => offsetLabel(Number(v)))}
						onchange={(v) => save({ ...s, sunriseOffsetMin: Number(v) }, 'sunrise-offset')}
					/>
				</li>
			{/if}
			<SettingRow
				icon="moon"
				label={m.shutters_close_at_sunset()}
				checked={s.closeAtSunset}
				busy={g.is('sunset')}
				onchange={(on) => save(toggled(s, 'sunset', on), 'sunset')}
				hint={s.closeAtSunset && cover.nextClose ? m.shutters_next_close({ when: dayAndTime(cover.nextClose) }) : undefined}
			/>
			{#if s.closeAtSunset}
				<li class="offset">
					<Select
						label={m.shutters_offset()}
						value={String(s.sunsetOffsetMin)}
						options={options(OFFSET_VALUES, (v) => offsetLabel(Number(v)))}
						onchange={(v) => save({ ...s, sunsetOffsetMin: Number(v) }, 'sunset-offset')}
					/>
				</li>
			{/if}
		</ul>
	{:else if place.data}
		<p class="hint">{m.place_needed()}</p>
	{/if}
</section>

<style>
	.sun {
		display: grid;
		gap: var(--s-3);
	}
	.offset {
		padding: var(--s-1) 0 var(--s-3) calc(var(--lead) + var(--s-3));
		border-bottom: 1px solid var(--line);
	}
</style>
