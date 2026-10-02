<script lang="ts">
	// Where the house is: a town looked up by name (Open-Meteo geocoding, through the backend),
	// picked from a short list. Typing waits a moment before asking, and asks once at a time.
	import { m } from '#lib/paraglide/messages.js';
	import { shuttersApi, type Place } from '#lib/api.ts';
	import { locale } from '#lib/i18n.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';

	/** Wait this long after the last key before asking (a town name is typed in one go). */
	const TYPING_PAUSE = 400;

	let { place, onpicked }: { place: Place | null; onpicked: (place: Place) => void } = $props();

	let editing = $state(false);
	let query = $state('');
	let results = $state.raw<Place[] | null>(null);
	const g = new Gesture();
	const id = $props.id();

	$effect(() => {
		const q = query.trim();
		if (q.length < 2) return void (results = null);
		const timer = setTimeout(() => void g.run(() => shuttersApi.searchPlaces(q, locale()), (r) => (results = r.places), 'search'), TYPING_PAUSE);
		return () => clearTimeout(timer);
	});

	function pick(p: Place) {
		return g.run(
			() => shuttersApi.setPlace(p),
			(r) => {
				ui.toast(m.place_saved({ name: r.place.name }));
				editing = false;
				query = '';
				results = null;
				onpicked(r.place);
			},
			'save'
		);
	}
</script>

<div class="place">
	<div class="head">
		<span class="label" id="{id}-title">{m.place_title()}</span>
		{#if place && !editing}
			<button class="link-btn" onclick={() => (editing = true)}>{m.common_edit()}</button>
		{/if}
	</div>
	{#if place && !editing}
		<p class="current"><Icon name="house" />{place.name}</p>
	{:else}
		<p class="hint">{m.place_hint()}</p>
		<div class="field">
			<label for="{id}-search">{m.place_search()}</label>
			<input id="{id}-search" bind:value={query} autocomplete="address-level2" aria-describedby="{id}-title" />
		</div>
		{#if results}
			{#if results.length}
				<ul class="list results" aria-label={m.place_search()}>
					{#each results as r (`${r.latitude},${r.longitude}`)}
						<li><button class="list-row option" disabled={g.is('save')} onclick={() => pick(r)}><span class="lead"><Icon name="house" size={16} /></span><span class="title">{r.name}</span></button></li>
					{/each}
				</ul>
			{:else}
				<p class="hint" role="status">{m.place_none()}</p>
			{/if}
		{/if}
	{/if}
</div>

<style>
	.place { display: grid; gap: var(--s-2); }
	.head { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s-3); }
	.label { font: var(--t-label); }
	.current { display: flex; align-items: center; gap: var(--s-2); margin: 0; }
	.results { display: grid; }
	.option { grid-template-columns: 20px minmax(0, 1fr); }
</style>
