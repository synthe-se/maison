<script lang="ts">
	// The ordered actions of a scene (SceneEditor): each action's row (ActionRow), moved or
	// removed with buttons, the focus following it (WCAG 2.4.3), and « Ajouter une action ».
	// The editor around it checks and saves.
	import { tick } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import Icon from '#lib/components/Icon.svelte';
	import ActionRow from './ActionRow.svelte';
	import type { Action } from './api.ts';
	import { ACTION_TYPES, defaultAction, toRow, type ActionType, type Row, type Sources } from './actions.ts';

	interface Props {
		rows: Row[];
		sources: Sources;
		/** The id of the problem said about the list (it describes the list and the add button). */
		problem?: string;
		/** Any change: the editor marks itself touched. */
		onedit: () => void;
	}
	let { rows = $bindable(), sources, problem, onedit }: Props = $props();
	const FIRST = (Object.keys(ACTION_TYPES) as ActionType[])[0];
	const id = $props.id();
	let addButton = $state<HTMLButtonElement | null>(null);

	function change(i: number, action: Action) {
		rows[i].action = action;
		onedit();
	}

	async function move(i: number, direction: -1 | 1) {
		const [row] = rows.splice(i, 1);
		rows.splice(i + direction, 0, row);
		onedit();
		const to = i + direction;
		ui.say(m.action_moved({ n: to + 1 }));
		// the focus follows the action, on the same arrow (at an end it stays, saying why it cannot)
		await tick();
		const which = direction === -1 ? 'up' : 'down';
		document.getElementById(`${id}-actions`)?.children[to]?.querySelector<HTMLButtonElement>(`[id$='-${which}']`)?.focus();
	}

	async function remove(i: number) {
		rows.splice(i, 1);
		onedit();
		ui.say(m.action_removed());
		await refocus(addButton);
	}

	function add() {
		rows.push(toRow(defaultAction(FIRST, sources)));
		onedit();
	}

	/** Puts the focus on what the problem is about: the first field still to fill, else « Ajouter ». */
	export function focusProblem() {
		return refocus(document.getElementById(`${id}-actions`)?.querySelector<HTMLElement>('[aria-invalid="true"], select, input'), addButton);
	}
</script>

<section class="stack" aria-labelledby="{id}-title">
	<h3 id="{id}-title" class="label">{m.actions_title()}</h3>
	<p class="hint">{m.actions_hint()}</p>
	{#if rows.length === 0}<p class="hint">{m.actions_missing()}</p>{/if}
	<ol class="stack roomy plain-list" id="{id}-actions" aria-describedby={problem}>
		{#each rows as row, i (row.id)}
			<ActionRow
				action={row.action}
				index={i}
				count={rows.length}
				{sources}
				onchange={(a) => change(i, a)}
				onmove={(d) => move(i, d)}
				onremove={() => remove(i)}
			/>
		{/each}
	</ol>
	<div>
		<button type="button" class="btn" bind:this={addButton} aria-describedby={problem} onclick={add}
			><Icon name="plus" />{m.action_add()}</button
		>
	</div>
</section>

<style>
	.label {
		font-weight: 600;
		padding: 0;
	}
	.btn {
		min-height: var(--control-h);
	}
</style>
