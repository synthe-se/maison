<script lang="ts">
	// One button's binding: which key (captured from the remote, or typed), its label, repeat,
	// and the ordered actions. « Tester » runs the actions now without saving and says how each
	// one went; « Enregistrer » saves. A key already configured edits the existing binding.
	import { tick } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { irApi, type IrAction, type IrBinding, type IrTestResponse } from '#lib/api.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import ActionRow from './ActionRow.svelte';
	import { defaultAction, isComplete, summarize, type Sources } from './actions.ts';
	import { keyName } from './keys.ts';

	interface Props {
		/** The key being edited; undefined: a new binding, captured from the remote. */
		code?: number;
		keymap: Record<string, IrBinding>;
		sources: Sources;
		onsaved: () => void;
		oncancel: () => void;
	}
	let { code: fixed, keymap, sources, onsaved, oncancel }: Props = $props();

	/** The STB's events are read once a second while capturing (as the React page did). */
	const CAPTURE_POLL_MS = 1_000;

	type Row = { id: number; action: IrAction };
	let nextId = 0;
	const rows = (actions: IrAction[]): Row[] => actions.map((action) => ({ id: nextId++, action }));

	// svelte-ignore state_referenced_locally (the form starts from the binding, then is the person's)
	const initial: IrBinding | undefined = fixed === undefined ? undefined : keymap[String(fixed)];
	// svelte-ignore state_referenced_locally
	let codeText = $state(fixed === undefined ? '' : String(fixed));
	let label = $state(initial?.label ?? '');
	let repeat = $state(initial?.repeat ?? false);
	let actions = $state<Row[]>(rows(initial?.actions ?? []));
	// svelte-ignore state_referenced_locally
	let capturing = $state(fixed === undefined);
	let dirty = false;
	/** The binding the form was filled from (kept whole: `debounce_ms` and friends survive a save). */
	let base: IrBinding | undefined = initial;
	// two gestures: a test may still be running its actions when « Enregistrer » is pressed
	const testing = new Gesture();
	const saving = new Gesture();
	let test = $state<{ response: IrTestResponse; actions: IrAction[] } | null>(null);
	let problem = $state('');
	let addButton = $state<HTMLButtonElement | null>(null);
	const id = $props.id();

	const code = $derived.by(() => {
		const n = Number.parseInt(codeText, 10);
		return /^\d+$/.test(codeText.trim()) && Number.isInteger(n) ? n : null;
	});
	const alreadyMapped = $derived(code !== null && code !== fixed && keymap[String(code)] !== undefined);

	/** A captured or typed key that is already configured: its binding fills an untouched form. */
	function adopt(next: number | null) {
		if (next === null || next === fixed || dirty) return;
		const existing = keymap[String(next)];
		if (!existing) return;
		base = existing;
		label = existing.label ?? '';
		repeat = existing.repeat ?? false;
		actions = rows(existing.actions);
	}

	// capture: the first press newer than the events seen when it started (server clock)
	$effect(() => {
		if (!capturing) return;
		let baseline: string | null = null;
		let stopped = false;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const poll = async () => {
			if (document.visibilityState === 'visible') {
				try {
					const { events } = await irApi.recent();
					if (stopped) return;
					if (baseline === null) baseline = events[0]?.receivedAt ?? '';
					else {
						const press = events.find((e) => e.value === 1 && e.receivedAt > (baseline ?? ''));
						if (press) {
							capturing = false;
							codeText = String(press.code);
							adopt(press.code);
							haptic(CONFIRM);
							ui.say(m.remote_captured({ key: keyName(press.code) }));
							return;
						}
					}
				} catch (e) {
					if (stopped) return;
					capturing = false;
					ui.fail(e);
					return;
				}
			}
			if (!stopped) timer = setTimeout(poll, CAPTURE_POLL_MS);
		};
		void poll();
		return () => {
			stopped = true;
			clearTimeout(timer);
		};
	});

	function edited() {
		dirty = true;
		test = null;
		problem = '';
	}

	function change(i: number, action: IrAction) {
		actions[i].action = action;
		edited();
	}

	async function move(i: number, direction: -1 | 1) {
		const [row] = actions.splice(i, 1);
		actions.splice(i + direction, 0, row);
		edited();
		const to = i + direction;
		ui.say(m.remote_moved({ n: to + 1 }));
		// the focus follows the action; at an end, to the arrow still usable
		await tick();
		const atEnd = (direction === -1 && to === 0) || (direction === 1 && to === actions.length - 1);
		const which = direction === -1 ? (atEnd ? 'down' : 'up') : atEnd ? 'up' : 'down';
		const list = document.getElementById(`${id}-actions`);
		list?.children[to]?.querySelector<HTMLButtonElement>(`[id$='-${which}']`)?.focus();
	}

	async function remove(i: number) {
		actions.splice(i, 1);
		edited();
		ui.say(m.remote_action_removed());
		await tick();
		addButton?.focus();
	}

	function add() {
		actions.push({ id: nextId++, action: defaultAction('nabaztag', sources) });
		edited();
	}

	/** Testing needs actions only; saving also needs the key. */
	function check(needCode: boolean): boolean {
		problem =
			needCode && code === null
				? m.remote_missing_code()
				: actions.length === 0
					? m.remote_missing_actions()
					: !actions.every((r) => isComplete(r.action))
						? m.remote_incomplete_actions()
						: '';
		if (problem) ui.say(problem);
		return !problem;
	}

	async function runTest() {
		if (!check(false)) return;
		test = null;
		const sent = actions.map((r) => r.action);
		await testing.run(
			() => irApi.test(sent),
			(response) => {
				test = { response, actions: sent };
				ui.say(response.success ? m.remote_test_passed() : m.remote_test_failed());
			}
		);
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (!check(true) || code === null) return;
		const key = code;
		await saving.run(
			() =>
				irApi.setBinding(key, {
					...base,
					actions: actions.map((r) => r.action),
					label: label.trim() || undefined,
					repeat
				}),
			() => {
				haptic(CONFIRM);
				ui.toast(m.remote_saved({ key: keyName(key) }));
				onsaved();
			}
		);
	}
</script>

<form class="editor" onsubmit={save} novalidate>
	<fieldset class="key">
		<legend class="label">{m.remote_key_code()}</legend>
		{#if fixed === undefined}
			<div class="actions">
				<button type="button" class="btn" aria-pressed={capturing} onclick={() => (capturing = !capturing)}>
					{#if capturing}<Icon name="loader-circle" class="spin" />{m.remote_stop_capture()}{:else}<Icon name="radio" />{m.remote_capture()}{/if}
				</button>
				<input
					id="{id}-code"
					class="code"
					inputmode="numeric"
					autocomplete="off"
					aria-label={m.remote_key_code()}
					value={codeText}
					oninput={(e) => {
						codeText = e.currentTarget.value;
						capturing = false;
						adopt(code);
					}}
				/>
			</div>
			{#if capturing}
				<p class="hint">{m.remote_capturing()}</p>
			{:else if code !== null}
				<p class="hint">{keyName(code)}</p>
			{/if}
		{:else}
			<p class="fixed">{keyName(fixed)} <span class="muted">{m.remote_key_number({ code: fixed })}</span></p>
		{/if}
		{#if alreadyMapped}<p class="warn">{m.remote_already_mapped()}</p>{/if}
	</fieldset>

	<div class="field">
		<label for="{id}-label">{m.remote_label()}</label>
		<input id="{id}-label" bind:value={label} oninput={edited} placeholder={m.remote_label_placeholder()} autocomplete="off" />
	</div>

	<div class="repeat">
		<Toggle
			label={m.remote_repeat()}
			checked={repeat}
			onchange={(v) => {
				repeat = v;
				edited();
			}}
		/>
		<p class="hint">{m.remote_repeat_hint()}</p>
	</div>

	<section class="list" aria-labelledby="{id}-actions-title">
		<h3 id="{id}-actions-title" class="label">{m.remote_actions()}</h3>
		<p class="hint">{m.remote_actions_hint()}</p>
		{#if actions.length === 0}<p class="hint">{m.remote_missing_actions()}</p>{/if}
		<ol class="rows" id="{id}-actions">
			{#each actions as row, i (row.id)}
				<ActionRow
					action={row.action}
					index={i}
					count={actions.length}
					{sources}
					onchange={(a) => change(i, a)}
					onmove={(d) => move(i, d)}
					onremove={() => remove(i)}
				/>
			{/each}
		</ol>
		<div>
			<button type="button" class="btn" bind:this={addButton} onclick={add}><Icon name="plus" />{m.remote_add_action()}</button>
		</div>
	</section>

	{#if test}
		<div class="callout" class:warn={!test.response.success}>
			<p class="outcome">
				<Icon name={test.response.success ? 'check' : 'triangle-alert'} />
				{test.response.success ? m.remote_test_passed() : m.remote_test_failed()}
			</p>
			<ol class="results">
				{#each test.response.results as result, i (i)}
					{@const ok = !result.startsWith('failed')}
					<li>
						<span>{test.actions[i] ? summarize(test.actions[i], sources) : ''}</span>
						<strong>{ok ? m.remote_test_item_ok() : m.remote_test_item_failed()}</strong>
						<span class="detail">{result.replace(/^(ok|failed):\s*/, '')}</span>
					</li>
				{/each}
			</ol>
		</div>
	{/if}

	<p class="form-error">{problem}</p>

	<div class="foot">
		<button type="button" class="btn" onclick={oncancel}>{m.common_cancel()}</button>
		<button type="button" class="btn" disabled={testing.is()} aria-describedby="{id}-test-hint" onclick={runTest}>
			{#if testing.is()}<Icon name="loader-circle" class="spin" />{:else}<Icon name="flask-conical" />{/if}{m.remote_test()}
		</button>
		<button type="submit" class="btn primary" disabled={saving.is()}>
			{#if saving.is()}<Icon name="loader-circle" class="spin" />{:else}<Icon name="save" />{/if}{m.common_save()}
		</button>
	</div>
	<p class="hint" id="{id}-test-hint">{testing.is() ? m.remote_testing() : m.remote_test_hint()}</p>
</form>

<style>
	.editor { display: grid; gap: var(--s-5); }
	.key { border: 0; padding: 0; margin: 0; display: grid; gap: var(--s-2); min-width: 0; }
	.label { font: var(--t-label); font-weight: 600; margin: 0; padding: 0; }
	.code { width: 8rem; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
	.fixed { margin: 0; font: var(--t-body); font-weight: 600; }
	.warn { margin: 0; font: var(--t-secondary); color: var(--warn-text); }
	.field input { width: 100%; min-width: 0; }
	.repeat { display: grid; gap: var(--s-1); }
	.list { display: grid; gap: var(--s-2); }
	.rows { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-3); }
	.outcome { display: flex; align-items: center; gap: var(--s-2); font-weight: 600; }
	.results { margin: 0; padding-left: var(--s-5); display: grid; gap: var(--s-2); font: var(--t-secondary); }
	.results li > * { display: block; }
	.detail { color: var(--ink-muted); font-family: ui-monospace, SFMono-Regular, Menlo, monospace; overflow-wrap: anywhere; }
	.foot { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--s-2); }
	.foot .btn { justify-content: center; min-height: var(--control-h); padding-inline: var(--s-2); }
	.btn { min-height: var(--control-h); }
</style>
