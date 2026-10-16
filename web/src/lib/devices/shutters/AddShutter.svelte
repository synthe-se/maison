<script lang="ts">
	// Adding a shutter (an admin's): Matter commissioning with the code from the phone's
	// ecosystem and a name, in a sheet. One call of up to a minute or so: its elapsed time is
	// said on screen (§ 6); a refusal is most often the code (wrong, or the window closed): said
	// under it, the focus back on it.
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import { Elapsed } from '#lib/elapsed.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import TextField from '#lib/components/TextField.svelte';
	import { missingFields } from '#lib/form.ts';
	import { shuttersApi, type Shutter } from './api.ts';

	let { open = $bindable(false), onadded }: { open?: boolean; onadded: (cover: Shutter) => unknown } = $props();

	const EMPTY = { code: '', name: '' };
	const draft = new Draft({ ...EMPTY });
	const form = $derived(draft.current);
	/** What the form says is missing, under its field. */
	let missing = $state<{ code?: string; name?: string }>({});
	const pairing = new Gesture();
	const elapsed = new Elapsed(() => pairing.is());
	let codeInput = $state<HTMLInputElement>();
	let nameInput = $state<HTMLInputElement>();

	function close() {
		open = false;
		draft.reset({ ...EMPTY });
		missing = {};
	}

	function pair(e: SubmitEvent) {
		e.preventDefault();
		const code = form.code.trim();
		const name = form.name.trim();
		const { errors, first } = missingFields({ code, name }, { code: m.shutters_code_missing, name: m.shutters_name_missing });
		missing = errors;
		if (first) return void (first === 'code' ? codeInput : nameInput)?.focus();
		return pairing.run(
			() => shuttersApi.commission(code, name),
			async (r) => {
				haptic(CONFIRM);
				ui.toast(m.shutters_added({ name: r.cover.name }));
				close();
				await onadded(r.cover);
			},
			'pair',
			{ field: () => codeInput }
		);
	}
</script>

<Sheet {open} onclose={close} {draft} title={m.shutters_add()} description={m.shutters_commission_hint()}>
	<form class="pair" onsubmit={pair} novalidate>
		<TextField
			label={m.shutters_code()}
			bind:input={codeInput}
			bind:value={form.code}
			error={missing.code || pairing.error}
			inputmode="numeric"
			autocomplete="off"
			placeholder="3497-011-2332"
		/>
		<TextField
			label={m.common_name()}
			bind:input={nameInput}
			bind:value={form.name}
			error={missing.name}
			placeholder={m.shutters_name_placeholder()}
		/>
		<div class="actions end">
			{#if pairing.is()}<span class="hint">{m.shutters_commissioning_elapsed({ seconds: elapsed.seconds })}</span>{/if}
			<button type="button" class="btn" onclick={close}>{m.common_cancel()}</button>
			<button class="btn primary" {...pending(pairing.is())}>
				<Icon name="plus" busy={pairing.is()} />{m.action_pair()}
			</button>
		</div>
	</form>
</Sheet>

<style>
	.pair {
		display: grid;
		gap: var(--s-4);
	}
	.pair .btn {
		min-height: var(--control-h);
	}
</style>
