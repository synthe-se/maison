<script lang="ts">
	// What an admin holds of the frame's photos: adding them (PhotoDrop), each one as a thumbnail in
	// the frame's portrait shape, removing one asks first (§ 6), then the focus goes to the list's
	// title.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, removeRow } from '#lib/gesture.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { photosApi } from './api.ts';
	import { photos as photosData } from './data.ts';
	import PhotoDrop from './PhotoDrop.svelte';

	const photos = live(photosData);
	const shown = $derived(photos.data?.photos ?? []);
	const g = new Gesture();
	let title = $state<HTMLElement>();

	const remove = (id: string) =>
		removeRow(g, id, () => photosApi.remove(id), { said: m.photos_removed(), list: photos, title: () => title });
</script>

<Group
	id="photos-title"
	title={m.photos_list_title()}
	fact={shown.length ? m.photos_count({ count: shown.length }) : undefined}
	bind:heading={title}
>
	<PhotoDrop onadded={() => photos.refresh()} />
	<Loaded value={photos} empty={shown.length === 0} emptyText={m.photos_none()} emptyHint={m.photos_none_hint()}>
		<ul class="grid plain-list">
			{#each shown as photo, i (photo.id)}
				{@const alt = m.photos_alt({ number: i + 1 })}
				<li>
					<img src={photosApi.src(photo.id)} {alt} loading="lazy" />
					<ConfirmDialog
						label={m.common_remove()}
						ariaLabel={m.common_remove_named({ name: alt })}
						ghost
						icon="trash"
						title={m.photos_remove_title()}
						description={m.photos_remove_consequence()}
						action={m.photos_remove_action()}
						busy={g.is(photo.id)}
						onconfirm={() => remove(photo.id)}
					/>
				</li>
			{/each}
		</ul>
	</Loaded>
</Group>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
		gap: var(--s-3);
		margin-top: var(--s-4);
	}
	li {
		display: grid;
		gap: var(--s-1);
		min-width: 0;
	}
	img {
		display: block;
		width: 100%;
		aspect-ratio: 5 / 8;
		object-fit: cover;
		border-radius: var(--radius-m);
		background: var(--ground-raised);
	}
</style>
