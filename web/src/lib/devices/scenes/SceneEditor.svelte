<script lang="ts">
	// One scene's form, in a sheet (an admin's): its name, its icon, its ordered actions (the
	// remote configurator's own list, ActionList: a scene never holds another scene), saved
	// whole; an existing scene can be deleted (confirmed: it cannot be undone, § 6).
	import { m } from '#lib/paraglide/messages.js';
	import { MAX_NAME } from '#lib/api.ts';
	import type { Draft } from '#lib/draft.svelte.ts';
	import { options } from '#lib/options.ts';
	import { scenesApi, type Scene } from './api.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Select from '#lib/components/Select.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import ActionList from '#lib/devices/remote/ActionList.svelte';
	import { isComplete, SCENE_TYPES, type Sources } from '#lib/devices/remote/actions.ts';
	import { SCENE_ICONS, slug, type SceneForm } from './scenes.ts';

	/** The backend keeps 1 to 20 actions per scene. */
	const MAX_ACTIONS = 20;

	interface Props {
		/** The scene to edit, or a template / a blank one to create (`isNew`). */
		scene: Scene;
		isNew: boolean;
		/** The ids already taken (a new scene's id must be its own). */
		taken: string[];
		sources: Sources;
		/** The form (the sheet reads whether it changed). */
		draft: Draft<SceneForm>;
		onsaved: () => void;
		ondeleted: () => void;
		oncancel: () => void;
	}
	let { scene, isNew, taken, sources, draft, onsaved, ondeleted, oncancel }: Props = $props();

	const form = $derived(draft.current);
	let problem = $state('');
	let problemAt = $state<'name' | 'actions' | null>(null);
	let nameInput = $state<HTMLInputElement | null>(null);
	let list = $state<ReturnType<typeof ActionList>>();
	const saving = new Gesture();
	const removing = new Gesture();
	const id = $props.id();
	const ICONS = SCENE_ICONS.map((i) => i.icon);
	const ICON_LABEL = (icon: string) => SCENE_ICONS.find((i) => i.icon === icon)?.label() ?? icon;

	function edited() {
		problem = '';
		problemAt = null;
	}

	function check(): boolean {
		const missingName = !form.name.trim();
		problem = missingName
			? m.scenes_missing_name()
			: form.rows.length === 0
				? m.remote_missing_actions()
				: form.rows.length > MAX_ACTIONS
					? m.scenes_too_many({ count: MAX_ACTIONS })
					: !form.rows.every((r) => isComplete(r.action))
						? m.remote_incomplete_actions()
						: '';
		problemAt = !problem ? null : missingName ? 'name' : 'actions';
		if (problemAt === 'name') void refocus(nameInput);
		else if (problemAt === 'actions') void list?.focusProblem();
		return !problem;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (!check()) return;
		const saved: Scene = {
			id: isNew && taken.includes(scene.id) ? slug(form.name, taken) : scene.id,
			name: form.name.trim(),
			icon: form.icon,
			actions: form.rows.map((r) => r.action)
		};
		await saving.run(
			() => scenesApi.save(saved),
			() => {
				haptic(CONFIRM);
				ui.toast(m.scenes_saved({ name: saved.name }));
				onsaved();
			},
			'',
			{ field: () => nameInput }
		);
	}

	const remove = () =>
		removing.run(
			() => scenesApi.remove(scene.id),
			() => {
				ui.toast(m.scenes_deleted({ name: scene.name }));
				ondeleted();
			}
		);
</script>

<form class="editor" onsubmit={save} novalidate>
	<div class="field">
		<label for="{id}-name">{m.common_name()}</label>
		<input
			id="{id}-name"
			bind:this={nameInput}
			bind:value={form.name}
			oninput={edited}
			maxlength={MAX_NAME}
			autocomplete="off"
			placeholder={m.scenes_template_leave()}
			aria-invalid={problemAt === 'name' || saving.error ? 'true' : undefined}
			aria-describedby="{id}-problem"
		/>
	</div>

	<Select
		label={m.scenes_icon()}
		value={form.icon}
		options={options(ICONS, ICON_LABEL)}
		onchange={(v) => {
			form.icon = v;
			edited();
		}}
	/>

	<ActionList
		bind:this={list}
		bind:rows={form.rows}
		{sources}
		types={SCENE_TYPES}
		problem={problemAt === 'actions' ? `${id}-problem` : undefined}
		onedit={edited}
	/>

	<p class="form-error" id="{id}-problem">{problem || saving.error}</p>

	<div class="foot">
		{#if !isNew}
			<ConfirmDialog
				danger
				icon="trash"
				label={m.common_delete()}
				ariaLabel={m.scenes_delete({ name: scene.name })}
				title={m.scenes_delete_title({ name: scene.name })}
				description={m.scenes_delete_description()}
				action={m.scenes_delete({ name: scene.name })}
				busy={removing.is()}
				onconfirm={remove}
			/>
		{/if}
		<span class="grow"></span>
		<button type="button" class="btn" onclick={oncancel}>{m.common_cancel()}</button>
		<button type="submit" class="btn primary" {...pending(saving.is())}>
			<Icon name="save" busy={saving.is()} />{m.common_save()}
		</button>
	</div>
</form>

<style>
	.editor {
		display: grid;
		gap: var(--s-5);
	}
	.foot {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-2);
		align-items: center;
	}
	.foot .grow {
		flex: 1;
	}
	.foot :global(.btn) {
		min-height: var(--control-h);
	}
</style>
