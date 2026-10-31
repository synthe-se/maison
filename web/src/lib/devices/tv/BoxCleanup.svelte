<script lang="ts">
	// Clearing the box (an admin's, in its settings): what the backend's table cuts, as the box
	// stands, why each, and the way back. Every package stays on the system partition, so both
	// gestures are safe and need no confirmation (docs/box.md). Google's home goes only once ours
	// is there: until then cleaning cannot act, and says why.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, gate, pending } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { androidTvApi, type CleanWhy } from './api.ts';
	import { boxCleanup } from './data.ts';

	const WHY: Record<CleanWhy, () => string> = {
		vendor_agent: m.box_clean_why_vendor_agent,
		home_screen: m.box_clean_why_home_screen,
		assistant: m.box_clean_why_assistant,
		casting: m.box_clean_why_casting,
		unused_app: m.box_clean_why_unused_app
	};

	const id = $props.id();
	const cleanup = live(boxCleanup);
	const g = new Gesture();
	const packages = $derived((cleanup.data?.packages ?? []).filter((p) => p.state !== 'absent'));
	const cut = $derived(packages.filter((p) => p.state === 'cleaned').length);
	const whys = $derived([...new Set(packages.map((p) => p.why))]);
	const noHome = $derived(cleanup.data?.launcher === false);

	function run(action: 'clean' | 'restore', done: string) {
		haptic(CONFIRM);
		return g.run(
			() => androidTvApi.clean(action),
			(r) => {
				cleanup.update(() => r);
				ui.toast(done);
			},
			action
		);
	}
</script>

<section class="cleanup" aria-labelledby="{id}-title">
	<h3 id="{id}-title">{m.box_clean_title()}</h3>
	<Loaded value={cleanup}>
		<p class="hint">{m.box_clean_count({ cut, total: packages.length })}</p>
		<ul>
			{#each whys as why (why)}
				{@const inGroup = packages.filter((p) => p.why === why)}
				<li>
					<span>{WHY[why]()}</span>
					<span class="hint">{m.box_clean_group({ cut: inGroup.filter((p) => p.state === 'cleaned').length, total: inGroup.length })}</span>
				</li>
			{/each}
		</ul>
		{#if noHome}<p class="hint" id="{id}-why">{m.box_clean_no_home()}</p>{/if}
		<div class="actions">
			<button class="btn" {...gate(noHome && `${id}-why`, g.is('clean'))} onclick={() => !noHome && run('clean', m.box_clean_done())}>
				<Icon name="sparkles" busy={g.is('clean')} />{m.box_clean_action()}
			</button>
			<button class="btn" {...pending(g.is('restore'))} onclick={() => run('restore', m.box_clean_restored())}>
				<Icon name="corner-up-left" busy={g.is('restore')} />{m.box_clean_restore()}
			</button>
		</div>
	</Loaded>
</section>

<style>
	.cleanup {
		display: grid;
		gap: var(--s-2);
	}
	ul {
		display: grid;
		gap: var(--s-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: flex;
		justify-content: space-between;
		gap: var(--s-2);
	}
</style>
