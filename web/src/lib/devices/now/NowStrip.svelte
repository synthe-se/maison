<script lang="ts">
	// « Maintenant », the top of the dashboard: one row of chips summing up the house (the Tempo
	// price in force until when, the lamps lit, the next shutter move, what needs a hand, the
	// devices out of reach). A summary that duplicates the groups and never reorders them: each
	// chip leads to its group (scrolled, focused). Same values as the groups (the same live()
	// keys: no request of its own), in a fixed order; compact (two lines at most on a phone).
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { hhmm, num } from '#lib/i18n.svelte.ts';
	import { time } from '#lib/clock.svelte.ts';
	import { TEMPO } from '#lib/devices/tempo/colors.ts';
	import { today } from '#lib/devices/tempo/data.ts';
	import { priceNow, redPeak } from '#lib/devices/tempo/price.ts';
	import { hue, zigbee } from '#lib/devices/lamps/lamp.ts';
	import { plugs as plugList } from '#lib/devices/meross/data.ts';
	import { devices } from '#lib/devices/cats/data.ts';
	import { covers as coverList } from '#lib/devices/shutters/data.ts';
	import { moveSentence, nextEvent } from '#lib/devices/shutters/sun.ts';
	import { refocus } from '#lib/focus.ts';
	import Chip from './Chip.svelte';
	import CatChip from './CatChip.svelte';
	import PlugChip from './PlugChip.svelte';

	const tempo = live(today);
	const hueList = live(hue.list);
	const zigbeeList = live(zigbee.list);
	const plugs = live(plugList);
	const cats = live(devices);
	const covers = live(coverList);

	const price = $derived(tempo.data ? priceNow(time.now, tempo.data) : null);

	const lamps = $derived([
		...(hueList.data?.lamps ?? []).map((l) => ({ on: l.state.isOn, reachable: l.connected })),
		...(zigbeeList.data?.lamps ?? []).map((l) => ({ on: l.state.isOn, reachable: l.reachable }))
	]);
	const lit = $derived(lamps.filter((l) => l.on && l.reachable).length);

	/** The soonest scheduled move not skipped, across the shutters. */
	const next = $derived(
		(covers.data?.covers ?? [])
			.filter((c) => c.online && nextEvent(c) && !nextEvent(c)!.skipped)
			.toSorted((a, b) => new Date(nextEvent(a)!.at).getTime() - new Date(nextEvent(b)!.at).getTime())[0]
	);

	// two lines at most on a phone: what does not fit waits behind « +2 » (the facts first, what
	// needs a hand last), shown on demand; hidden chips are hidden for everyone (`hidden`)
	const LINES = 2;
	let list = $state<HTMLUListElement>();
	let more = $state<HTMLLIElement>();
	let open = $state(false);
	let folded = $state(0);
	function fit() {
		if (!list || !more) return;
		const chips = [...list.children].filter((li): li is HTMLLIElement => li !== more);
		for (const li of chips) li.hidden = false;
		more.hidden = true;
		folded = 0;
		const lines = () =>
			new Set(
				chips
					.filter((li) => !li.hidden)
					.concat(more!.hidden ? [] : [more!])
					.map((li) => li.offsetTop)
			).size;
		if (open || lines() <= LINES) return;
		more.hidden = false;
		// the facts from the end first, then what needs a hand from the end
		const order = [
			...chips.filter((li) => !li.querySelector('.warn')).toReversed(),
			...chips.filter((li) => li.querySelector('.warn')).toReversed()
		];
		for (const li of order) {
			if (lines() <= LINES) break;
			li.hidden = true;
			folded++;
		}
	}
	/** Everything shown; the focus goes to the first chip that was folded (its button is gone). */
	async function unfold() {
		const first = [...(list?.children ?? [])].find((li) => (li as HTMLElement).hidden && li !== more);
		open = true;
		await refocus(() => first?.querySelector('button'));
	}
	$effect(() => {
		if (!list) return;
		void open;
		fit();
		const resized = new ResizeObserver(() => fit());
		const changed = new MutationObserver(() => fit());
		resized.observe(list);
		changed.observe(list, { childList: true, subtree: true, characterData: true });
		return () => {
			resized.disconnect();
			changed.disconnect();
		};
	});
</script>

<section class="now" aria-labelledby="now-title">
	<h2 id="now-title" class="sr-only">{m.now_title()}</h2>
	<ul class="chips plain-list" bind:this={list}>
		{#if price}
			<Chip
				text={m.now_tempo({
					color: TEMPO[price.color].name(),
					period: price.period === 'peak' ? m.tempo_hp() : m.tempo_hc(),
					cents: num(price.price * 100, 1),
					time: hhmm(price.until)
				})}
				to="tempo-title"
				warn={redPeak(price)}
			/>
		{/if}
		{#if lit}<Chip text={m.now_lamps_on({ count: lit })} to="lamps-title" />{/if}
		{#if next}<Chip text={moveSentence(next) ?? ''} to="shutters-title" />{/if}
		{#each cats.data?.devices ?? [] as d (d.id)}
			{#if d.type !== 'unknown'}<CatChip device={d} />{/if}
		{/each}
		{#each plugs.data?.devices ?? [] as p (p.id)}<PlugChip plug={p} {price} />{/each}
		<li bind:this={more} hidden>
			<button class="pill-btn now-chip" aria-expanded={open} aria-label={m.now_more({ count: folded })} onclick={unfold}>+{folded}</button>
		</li>
	</ul>
</section>

<style>
	.now {
		margin: 0 0 var(--s-4);
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-1);
	}
	.chips:not(:has(li:not([hidden]))) {
		display: none;
	}
	.chips :global(.now-chip) {
		font: var(--t-tiny);
		gap: var(--s-1);
		padding: 0 var(--s-2);
		font-variant-numeric: tabular-nums;
	}
	.chips :global(.now-chip.warn) {
		background: var(--blocked-wash);
		color: var(--warn-text);
		border-color: var(--blocked);
	}
</style>
