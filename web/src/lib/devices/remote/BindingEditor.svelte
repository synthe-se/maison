<script lang="ts">
	// One button's binding: which key (captured from the remote, or typed), its label, repeat,
	// and the ordered actions. « Tester » runs the actions now without saving and says how each
	// one went; « Enregistrer » saves. A key already configured edits the existing binding.
	import { tick } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { irApi, type IrAction, type IrBinding, type IrEvent, type IrTestResponse } from '#lib/api.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
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
		/** Touched since it opened: a key captured fills only an untouched form, and the sheet
		 * asks before losing it. */
		dirty?: boolean;
	}
	let { code: fixed, keymap, sources, onsaved, oncancel, dirty = $bindable(false) }: Props = $props();

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
	/** The binding the form was filled from (kept whole: `debounce_ms` and friends survive a save). */
	let base: IrBinding | undefined = initial;
	// two gestures: a test may still be running its actions when « Enregistrer » is pressed
	const testing = new Gesture();
	const saving = new Gesture();
	let test = $state<{ response: IrTestResponse; actions: IrAction[] } | null>(null);
	let problem = $state('');
	let addButton = $state<HTMLButtonElement | null>(null);
	let codeInput = $state<HTMLInputElement | null>(null);
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

	// capture: the newest press numbered after the newest event seen when it started. By the
	// backend's sequence, never by its clock: an NTP step back on the Pi (no RTC) would hide
	// every new press. A sequence lower than the baseline means the backend restarted: all new.
	$effect(() => {
		if (!capturing) return;
		/** undefined: not read yet; 0: the list was empty. */
		let baseline: number | undefined = undefined;
		let stopped = false;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const poll = async () => {
			if (document.visibilityState === 'visible') {
				try {
					const { events } = await irApi.recent();
					if (stopped) return;
					if (baseline === undefined) baseline = events[0]?.seq ?? 0;
					else {
						const since = (events[0]?.seq ?? 0) < baseline ? 0 : baseline;
						const press = events.find((e) => e.seq > since && e.value === 1);
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

	/** What the problem is about: the field it is said under, and that takes the focus. */
	let problemAt = $state<'code' | 'actions' | null>(null);

	/** Testing needs actions only; saving also needs the key. */
	function check(needCode: boolean): boolean {
		const missingCode = needCode && code === null;
		problem = missingCode
			? m.remote_missing_code()
			: actions.length === 0
				? m.remote_missing_actions()
				: !actions.every((r) => isComplete(r.action))
					? m.remote_incomplete_actions()
					: '';
		problemAt = !problem ? null : missingCode ? 'code' : 'actions';
		if (problem) {
			// the field is described by the problem, and read when it takes the focus
			if (missingCode && codeInput) void refocus(codeInput);
			else void refocus(document.getElementById(`${id}-actions`)?.querySelector<HTMLElement>('[aria-invalid="true"], select, input') ?? addButton);
		}
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
				<!-- a plain button: its words change with what it does (APG: no aria-pressed then) -->
				<button type="button" class="btn" onclick={() => (capturing = !capturing)}>
					<Icon name="radio" busy={capturing} />{capturing ? m.remote_stop_capture() : m.remote_capture()}
				</button>
				<input
					id="{id}-code"
					bind:this={codeInput}
					class="code"
					aria-invalid={problemAt === 'code' ? 'true' : undefined}
					aria-describedby={problemAt === 'code' ? `${id}-problem` : undefined}
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
		{#if alreadyMapped}<p class="warn-text">{m.remote_already_mapped()}</p>{/if}
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
		<ol class="rows" id="{id}-actions" aria-describedby={problemAt === 'actions' ? `${id}-problem` : undefined}>
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
			<button
				type="button"
				class="btn"
				bind:this={addButton}
				aria-describedby={problemAt === 'actions' ? `${id}-problem` : undefined}
				onclick={add}><Icon name="plus" />{m.remote_add_action()}</button
			>
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

	<p class="form-error" id="{id}-problem">{problem}</p>

	<div class="foot">
		<button type="button" class="btn" onclick={oncancel}>{m.common_cancel()}</button>
		<button type="button" class="btn" {...pending(testing.is())} aria-describedby="{id}-test-hint" onclick={runTest}>
			<Icon name="flask-conical" busy={testing.is()} />{m.remote_test()}
		</button>
		<button type="submit" class="btn primary" {...pending(saving.is())}>
			<Icon name="save" busy={saving.is()} />{m.common_save()}
		</button>
	</div>
	<p class="hint" id="{id}-test-hint">{testing.is() ? m.remote_testing() : m.remote_test_hint()}</p>
</form>

<style>
	.editor { display: grid; gap: var(--s-5); }
	.key { border: 0; padding: 0; margin: 0; display: grid; gap: var(--s-2); min-width: 0; }
	.label { font-weight: 600; padding: 0; }
	.code { width: 8rem; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
	.fixed { margin: 0; font: var(--t-body); font-weight: 600; }

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
