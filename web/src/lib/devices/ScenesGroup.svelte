<script lang="ts">
	// « Scènes » near the top of the dashboard: one button per scene (« Je pars », « Nuit »,
	// « Film »), each a Gesture whose outcome is said, a failed action named. After them, for an
	// admin, « Créer »: a sheet « Nouvelle scène » that can start from a template filled from the
	// house's devices; the pencil in the head edits the existing ones. Members see the scenes only. `/?scene=<id>` (the app's shortcuts) asks
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
	/** The templates a new scene may start from: those the house does not have yet. */
	const offered = $derived(templates(sources.current).filter((t) => !all.some((s) => s.id === t.id)));
	/** A new scene starts again from `t` (the form is refilled from it). */
	function start(t: Scene) {
		if (editing) editing = { ...editing, scene: t, draft: new Draft(sceneForm(t)) };
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
		{#if all.length || list.data}
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
				{#if session.admin}
					<li>
						<button class="btn ghost scene" aria-haspopup="dialog" onclick={(e) => edit(blank(), true, e)}
							><Icon name="plus" />{m.scenes_add()}</button
						>
					</li>
				{/if}
			</ul>
		{/if}
	</Group>
{/if}

{#snippet manage()}
	<!-- the edit buttons only when asked: one row of scenes otherwise -->
	<button class="icon-btn" aria-label={m.scenes_manage()} aria-pressed={managing} onclick={() => (managing = !managing)}
		><Icon name="pencil" /></button
	>
{/snippet}

<Sheet
	open={editing !== null}
	onclose={() => closed(false)}
	draft={editing?.draft}
	title={editing?.isNew ? m.scenes_new() : m.scenes_edit({ name: editing?.scene.name ?? '' })}
	description={m.scenes_editor_description()}
>
	{#if editing}
		{#if editing.isNew && offered.length}
			<div class="from" role="group" aria-labelledby="scenes-from">
				<span id="scenes-from" class="label">{m.scenes_from_template()}</span>
				<div class="row">
					{#each offered as t (t.id)}
						<button class="btn" aria-pressed={editing.scene.id === t.id} onclick={() => start(t)}><Icon name={iconOf(t)} />{t.name}</button>
					{/each}
				</div>
			</div>
		{/if}
		{#key editing.scene}
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
		{/key}
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
	.from {
		display: grid;
		gap: var(--s-2);
		margin-bottom: var(--s-4);
	}
	.from .row {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-2);
	}
</style>
