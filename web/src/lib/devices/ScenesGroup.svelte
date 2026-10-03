<script lang="ts">
	// « Scènes » near the top of the dashboard: one button per scene (« Je pars », « Nuit »,
	// « Film »), each a Gesture whose outcome is said, a failed action named. Admins create,
	// edit and delete them in a sheet; with none yet, admins are offered three templates filled
	// from the house's devices, members see nothing. `/?scene=<id>` (the app's shortcuts) asks
	// first, then runs: a scene never runs silently.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import type { Scene } from './scenes/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import { liveSources, summarize } from './remote/actions.ts';
	import SceneEditor from './scenes/SceneEditor.svelte';
	import { iconOf, runScene, sceneForm, templates, type SceneForm } from './scenes/scenes.ts';
	import { scenes } from './scenes/data.ts';

	const list = live(scenes);
	const all = $derived(list.data?.scenes ?? []);
	const sources = liveSources();
	const g = new Gesture();
	let title = $state<HTMLElement>();

	/** The editor: closed, or a scene (new from a template or blank, or existing), its draft,
	 * and the button that opened it. */
	let editing = $state<{ scene: Scene; isNew: boolean; draft: Draft<SceneForm>; opener?: HTMLElement | null } | null>(null);
	/** An admin shows each scene's edit button. */
	let managing = $state(false);
	const blank = (): Scene => ({ id: 'scene', name: '', icon: 'sparkles', actions: [] });

	function edit(scene: Scene, isNew: boolean, e?: Event) {
		editing = { scene, isNew, draft: new Draft(sceneForm(scene)), opener: e?.currentTarget as HTMLElement | null };
	}
	async function closed(reread: boolean) {
		const opener = editing?.opener;
		editing = null;
		if (reread) await list.refresh();
		// the button that opened it, else (a scene deleted, its buttons gone) the group's title
		await refocus(opener, title);
	}

	// ── `/?scene=<id>`: asked first, then run ──
	const asked = $derived(page.url.searchParams.get('scene'));
	const askedScene = $derived(asked ? all.find((s) => s.id === asked) : undefined);
	let confirming = $state(false);
	/** The `?scene=` already asked about (asked once, whatever the answer). */
	let handled: string | null = null;
	$effect(() => {
		if (!asked || !list.data || handled === asked) return;
		handled = asked;
		if (askedScene) confirming = true;
		else {
			ui.toast(m.scenes_not_found({ id: asked }), { warn: true });
			void forget();
		}
	});
	// asked, then answered either way (« Lancer » or « Garder »): the address forgets it
	let wasAsking = false;
	$effect(() => {
		if (confirming) wasAsking = true;
		else if (wasAsking) {
			wasAsking = false;
			void forget();
		}
	});
	/** The address loses its `?scene=`: a reload never asks again. */
	const forget = () => goto('/', { replace: true });
</script>

{#if all.length || session.admin}
	<Group id="scenes-title" title={m.scenes_title()} bind:heading={title} actions={session.admin && all.length ? manage : undefined}>
		{#if all.length}
			<ul class="scenes plain-list">
				{#each all as s (s.id)}
					<li>
						<button class="btn scene" {...pending(g.is(s.id))} onclick={() => runScene(g, s)}>
							<Icon name={iconOf(s)} busy={g.is(s.id)} />{s.name}
						</button>
						{#if session.admin && managing}
							<button class="icon-btn" aria-label={m.scenes_edit({ name: s.name })} onclick={(e) => edit(s, false, e)}
								><Icon name="pencil" /></button
							>
						{/if}
					</li>
				{/each}
			</ul>
		{:else if list.data}
			<!-- no scene yet: an admin starts from the house's own devices -->
			<p class="hint">{m.scenes_none_hint()}</p>
			<ul class="scenes plain-list">
				{#each templates(sources.current) as t (t.id)}
					<li>
						<button class="btn scene" onclick={(e) => edit(t, true, e)}><Icon name="plus" />{m.scenes_create({ name: t.name })}</button>
					</li>
				{/each}
			</ul>
		{/if}
	</Group>
{/if}

{#snippet manage()}
	<!-- the edit buttons only when asked: one row of scenes otherwise -->
	<button class="icon-btn" aria-label={m.scenes_manage()} aria-pressed={managing} onclick={() => (managing = !managing)}
		><Icon name="pencil" /></button
	>
	<button class="icon-btn" aria-label={m.scenes_new()} onclick={(e) => edit(blank(), true, e)}><Icon name="plus" /></button>
{/snippet}

<Sheet
	open={editing !== null}
	onclose={() => closed(false)}
	draft={editing?.draft}
	title={editing?.isNew ? m.scenes_new() : m.scenes_edit({ name: editing?.scene.name ?? '' })}
	description={m.scenes_editor_description()}
>
	{#if editing}
		<SceneEditor
			scene={editing.scene}
			isNew={editing.isNew}
			taken={all.map((s) => s.id)}
			sources={sources.current}
			draft={editing.draft}
			onsaved={() => closed(true)}
			ondeleted={() => closed(true)}
			oncancel={() => closed(false)}
		/>
	{/if}
</Sheet>

{#if askedScene}
	<ConfirmDialog
		bind:open={confirming}
		title={m.scenes_run_title({ name: askedScene.name })}
		description={askedScene.actions.map((a) => summarize(a, sources.current)).join(' · ')}
		action={m.scenes_run({ name: askedScene.name })}
		onconfirm={() => {
			void runScene(g, askedScene);
		}}
	/>
{/if}

<style>
	.scenes {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-2);
	}
	.scenes li {
		display: flex;
		align-items: center;
		gap: var(--s-1);
	}
	.scene {
		min-height: var(--control-h);
	}
</style>
