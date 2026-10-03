<script lang="ts">
	// A Hue lamp's page: the shared lamp page, plus why it may not be connected and hiding it.
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { m } from '#lib/paraglide/messages.js';
	import { hueLampsApi } from '#lib/api.ts';
	import { live, refresh } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import LampPage from './LampPage.svelte';
	import { DETAIL_EVERY, fromHue, hue, type Lamp } from './lamp.ts';

	let { id }: { id: string } = $props();
	const status = live(untrack(() => hue.keys.detail(id)), () => hueLampsApi.status(id), DETAIL_EVERY);
	const raw = $derived(status.data?.lamp);
	const lamp = $derived(raw ? fromHue(raw) : undefined);

	const hiding = new Gesture();
	const hide = (l: Lamp) =>
		hiding.run(
			() => hueLampsApi.blacklist(l.id),
			async () => {
				ui.toast(m.hue_lamps_blacklisted({ name: l.name }));
				void refresh(hue.key);
				await goto('/');
			}
		);
</script>

<LampPage {lamp} loading={status.loading} driver={hue} rows={raw ? [[m.hue_lamps_address(), raw.address]] : []}>
	{#snippet notes(l: Lamp)}
		{#if !l.reachable && !l.connecting}
			<p class="callout warn">{m.hue_lamps_not_connected_description()}</p>
		{/if}
	{/snippet}

	{#snippet children(l: Lamp)}
		<AdminOnly reason={false}>
		<section aria-labelledby="hue-hide">
			<h2 id="hue-hide" class="group-title">{m.hue_lamps_blacklist()}</h2>
			<p class="hint">{m.hue_lamps_blacklist_section_description()}</p>
			<ConfirmDialog
				danger
				icon="ban"
				label={m.hue_lamps_blacklist()}
				title={m.hue_lamps_blacklist_title({ name: l.name })}
				description={m.hue_lamps_blacklist_description()}
				action={m.hue_lamps_blacklist()}
				pending={hiding.is()}
				onconfirm={() => hide(l)}
			/>
		</section>
		</AdminOnly>
	{/snippet}
</LampPage>

<style>
	section { display: grid; gap: var(--s-3); justify-items: start; }
</style>
