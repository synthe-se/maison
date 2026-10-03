<script lang="ts">
	// Where the house is: a town looked up by name (Open-Meteo geocoding, through the backend),
	// picked from its matches (Combobox). Typing waits a moment before asking; only the answer
	// to the last question counts (an older one arriving late, or failing, is dropped); the
	// search, its count and a failure are said where they happen (the field's description), not
	// in a toast. Changing the place is an admin's.
	import { m } from '#lib/paraglide/messages.js';
	import { locale } from '#lib/i18n.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import Combobox from '#lib/components/Combobox.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import { shuttersApi, type Place } from './api.ts';

	/** Wait this long after the last key before asking (a town name is typed in one go). */
	const TYPING_PAUSE = 400;

	let { place, onpicked }: { place: Place | null; onpicked: (place: Place) => void } = $props();

	let editing = $state(false);
	let query = $state('');
	let results = $state.raw<Place[] | null>(null);
	let open = $state(false);
	/** The searches (each its own key: a newer one never waits for an older); the choice. */
	const finder = new Gesture();
	const g = new Gesture();
	const id = $props.id();
	let editButton = $state<HTMLButtonElement>();
	/** The last question asked: an answer to an older one is dropped. */
	let asked = 0;
	const searching = $derived(finder.is());
	const key = (p: Place) => `${p.latitude},${p.longitude}`;

	$effect(() => {
		const q = query.trim();
		const lang = locale();
		const mine = ++asked;
		if (q.length < 2) {
			results = null;
			return;
		}
		const timer = setTimeout(
			() =>
				finder.run(
					() => shuttersApi.searchPlaces(q, lang),
					(r) => {
						if (mine !== asked) return;
						results = r.places;
						open = r.places.length > 0;
						ui.say(m.place_results({ count: r.places.length }));
					},
					`search-${mine}`,
					{ inline: true, refused: () => mine !== asked }
				),
			TYPING_PAUSE
		);
		return () => clearTimeout(timer);
	});

	function pick(value: string) {
		const p = results?.find((r) => key(r) === value);
		if (!p) return;
		return g.run(
			() => shuttersApi.setPlace(p),
			async (r) => {
				ui.say(m.place_saved({ name: r.place.name }));
				editing = false;
				query = '';
				results = null;
				onpicked(r.place);
				// the field is gone: the focus goes to the way to change it again
				await refocus(editButton);
			},
			'save'
		);
	}
</script>

<div class="place">
	<div class="head">
		<span class="label" id="{id}-title">{m.place_title()}</span>
		{#if place && !editing && session.admin}
			<button class="link-btn" bind:this={editButton} onclick={() => (editing = true)}>{m.common_edit()}</button>
		{/if}
	</div>
	{#if place && !editing}
		<p class="current"><Icon name="house" />{place.name}</p>
	{:else if !session.admin}
		<p class="hint">{m.place_admin()}</p>
	{:else}
		<p class="hint">{m.place_hint()}</p>
		<Combobox
			label={m.place_search()}
			bind:query
			bind:open
			options={(results ?? []).map((r) => ({ value: key(r), label: r.name }))}
			onpick={pick}
			icon="house"
			autocomplete="address-level2"
			describedby="{id}-title {id}-status"
			invalid={!!finder.error}
		/>
		<p class="hint" id="{id}-status" aria-busy={searching || g.is()}>
			{#if searching}{m.place_searching()}{:else if finder.error}<span class="warn-text">{finder.error}</span
				>{:else if results && !results.length}{m.place_none()}{/if}
		</p>
	{/if}
</div>

<style>
	.place {
		display: grid;
		gap: var(--s-2);
	}
	.head {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: var(--s-3);
	}
	.current {
		display: flex;
		align-items: center;
		gap: var(--s-2);
		margin: 0;
	}
</style>
