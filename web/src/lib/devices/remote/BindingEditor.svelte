<script lang="ts">
	// One button's binding: which key (captured from the remote, or typed), its label, repeat,
	// and the ordered actions, in a draft the sheet reads (it asks before losing a change).
	// « Tester » runs the actions now without saving and says how each one went (TestResult);
	// « Enregistrer » saves. A key already configured edits the existing binding; a captured or
	// typed key that is configured fills an untouched form.
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import type { Draft } from '#lib/draft.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import { irApi, type IrAction, type IrBinding, type IrTestResponse } from './api.ts';
	import ActionList from './ActionList.svelte';
	import KeyCapture from './KeyCapture.svelte';
	import TestResult from './TestResult.svelte';
	import { bindingForm, isComplete, type BindingForm, type Sources } from './actions.ts';
	import { keyName } from './keys.ts';

	interface Props {
		/** The key being edited; undefined: a new binding, captured from the remote. */
		code?: number;
		keymap: Record<string, IrBinding>;
		sources: Sources;
		draft: Draft<BindingForm>;
		onsaved: () => void;
		oncancel: () => void;
	}
	let { code: fixed, keymap, sources, draft, onsaved, oncancel }: Props = $props();

	const form = $derived(draft.current);
	// svelte-ignore state_referenced_locally (the form starts from the binding, then is the person's)
	let codeText = $state(fixed === undefined ? '' : String(fixed));
	// svelte-ignore state_referenced_locally
	let capturing = $state(fixed === undefined);
	/** The binding the form was filled from (kept whole: `debounceMs` and friends survive a save). */
	// svelte-ignore state_referenced_locally
	let base: IrBinding | undefined = fixed === undefined ? undefined : keymap[String(fixed)];
	// two gestures: a test may still be running its actions when « Enregistrer » is pressed
	const testing = new Gesture();
	const saving = new Gesture();
	let test = $state<{ response: IrTestResponse; actions: IrAction[] } | null>(null);
	let problem = $state('');
	/** What the problem is about: the field it is said under, and that takes the focus. */
	let problemAt = $state<'code' | 'actions' | null>(null);
	let list = $state<ReturnType<typeof ActionList>>();
	let codeInput = $state<HTMLInputElement | null>(null);
	const id = $props.id();

	const code = $derived.by(() => {
		const n = Number.parseInt(codeText, 10);
		return /^\d+$/.test(codeText.trim()) && Number.isInteger(n) ? n : null;
	});
	const alreadyMapped = $derived(code !== null && code !== fixed && keymap[String(code)] !== undefined);

	/** A captured or typed key that is already configured: its binding fills an untouched form. */
	function adopt(next: number | null) {
		if (next === null || next === fixed || draft.dirty) return;
		const existing = keymap[String(next)];
		if (!existing) return;
		base = existing;
		draft.reset(bindingForm(existing));
	}

	function captured(key: number) {
		capturing = false;
		codeText = String(key);
		adopt(key);
		haptic(CONFIRM);
		ui.say(m.remote_captured({ key: keyName(key) }));
	}

	function edited() {
		test = null;
		problem = '';
	}

	/** Testing needs actions only; saving also needs the key. */
	function check(needCode: boolean): boolean {
		const missingCode = needCode && code === null;
		problem = missingCode
			? m.remote_missing_code()
			: form.actions.length === 0
				? m.remote_missing_actions()
				: !form.actions.every((r) => isComplete(r.action))
					? m.remote_incomplete_actions()
					: '';
		problemAt = !problem ? null : missingCode ? 'code' : 'actions';
		if (problem) {
			// the field is described by the problem, and read when it takes the focus
			if (missingCode && codeInput) void refocus(codeInput);
			else void list?.focusProblem();
		}
		return !problem;
	}

	async function runTest() {
		if (!check(false)) return;
		test = null;
		const sent = form.actions.map((r) => r.action);
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
					actions: form.actions.map((r) => r.action),
					label: form.label.trim() || undefined,
					repeat: form.repeat
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
				<KeyCapture
					oncapture={captured}
					onerror={(e) => {
						capturing = false;
						ui.fail(e);
					}}
				/>
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
		<input id="{id}-label" bind:value={form.label} oninput={edited} placeholder={m.remote_label_placeholder()} autocomplete="off" />
	</div>

	<div class="repeat">
		<Toggle
			label={m.remote_repeat()}
			checked={form.repeat}
			onchange={(v) => {
				form.repeat = v;
				edited();
			}}
		/>
		<p class="hint">{m.remote_repeat_hint()}</p>
	</div>

	<ActionList
		bind:this={list}
		bind:rows={form.actions}
		{sources}
		problem={problemAt === 'actions' ? `${id}-problem` : undefined}
		onedit={edited}
	/>

	{#if test}<TestResult response={test.response} actions={test.actions} {sources} />{/if}

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
	.editor {
		display: grid;
		gap: var(--s-5);
	}
	.key {
		border: 0;
		padding: 0;
		margin: 0;
		display: grid;
		gap: var(--s-2);
		min-width: 0;
	}
	.label {
		font-weight: 600;
		padding: 0;
	}
	.code {
		width: 8rem;
		font-family: var(--font-mono);
	}
	.fixed {
		margin: 0;
		font: var(--t-body);
		font-weight: 600;
	}
	.repeat {
		display: grid;
		gap: var(--s-1);
	}
	.foot {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: var(--s-2);
	}
	.foot .btn {
		justify-content: center;
		padding-inline: var(--s-2);
	}
	.btn {
		min-height: var(--control-h);
	}
</style>
