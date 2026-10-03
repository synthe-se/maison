<script lang="ts">
	// Where the house is: a town looked up by name (Open-Meteo geocoding, through the backend),
	// picked from a short list. Typing waits a moment before asking; only the answer to the last
	// question counts (an older one arriving late never replaces it); the search, its count and
	// a failure are said where they happen, not in a toast. Changing the place is an admin's.
	import { m } from '#lib/paraglide/messages.js';
	import { shuttersApi, type Place } from '#lib/api.ts';
	import { locale } from '#lib/i18n.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { errorText } from '#lib/errors.ts';
	import { refocus } from '#lib/focus.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';

	/** Wait this long after the last key before asking (a town name is typed in one go). */
	const TYPING_PAUSE = 400;

	let { place, onpicked }: { place: Place | null; onpicked: (place: Place) => void } = $props();

	let editing = $state(false);
	let query = $state('');
	let results = $state.raw<Place[] | null>(null);
	let searching = $state(false);
	let searchError = $state('');
	const g = new Gesture();
	const id = $props.id();
	let editButton = $state<HTMLButtonElement>();
	/** The last question asked: an answer to an older one is dropped. */
	let asked = 0;

	$effect(() => {
		const q = query.trim();
		const lang = locale();
		const mine = ++asked;
		searchError = '';
		if (q.length < 2) {
			results = null;
			searching = false;
			return;
		}
		const timer = setTimeout(async () => {
			searching = true;
			try {
				const r = await shuttersApi.searchPlaces(q, lang);
				if (mine !== asked) return;
				results = r.places;
				ui.say(m.place_results({ count: r.places.length }));
			} catch (e) {
				if (mine !== asked) return;
				results = null;
				searchError = errorText(e);
			} finally {
				if (mine === asked) searching = false;
			}
		}, TYPING_PAUSE);
		return () => clearTimeout(timer);
	});

	function pick(p: Place) {
		return g.run(
			() => shuttersApi.setPlace(p),
			async (r) => {
				ui.say(m.place_saved({ name: r.place.name }));
				editing = false;
				query = '';
				results = null;
				onpicked(r.place);
				// the list and its field are gone: the focus goes to the way to change it again
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
		<div class="field">
			<label for="{id}-search">{m.place_search()}</label>
			<input
				id="{id}-search"
				bind:value={query}
				autocomplete="address-level2"
				aria-describedby="{id}-title {id}-status"
				aria-invalid={searchError ? 'true' : undefined}
			/>
			<p class="hint" id="{id}-status">
				{#if searching}{m.place_searching()}{:else if searchError}<span class="warn-text">{searchError}</span>{:else if results && !results.length}{m.place_none()}{/if}
			</p>
		</div>
		{#if results?.length}
			<ul class="list results" aria-label={m.place_search()} aria-busy={searching}>
				{#each results as r (`${r.latitude},${r.longitude}`)}
					<li>
						<button class="list-row option" {...pending(g.is('save'))} onclick={() => pick(r)}>
							<span class="lead"><Icon name="house" size={16} /></span><span class="title">{r.name}</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</div>

<style>
	.place { display: grid; gap: var(--s-2); }
	.head { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s-3); }
	.current { display: flex; align-items: center; gap: var(--s-2); margin: 0; }
	.results { display: grid; }
	.option { grid-template-columns: 20px minmax(0, 1fr); }
</style>
