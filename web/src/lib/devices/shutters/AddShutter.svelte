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
	import { shuttersApi, type Shutter } from './api.ts';

	let { open = $bindable(false), onadded }: { open?: boolean; onadded: (cover: Shutter) => unknown } = $props();
	const id = $props.id();

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
		missing = { code: code ? undefined : m.shutters_code_missing(), name: name ? undefined : m.shutters_name_missing() };
		if (missing.code || missing.name) return void (missing.code ? codeInput : nameInput)?.focus();
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
		<div class="field">
			<label for="{id}-code">{m.shutters_code()}</label>
			<input
				id="{id}-code"
				bind:this={codeInput}
				bind:value={form.code}
				inputmode="numeric"
				autocomplete="off"
				placeholder="3497-011-2332"
				aria-invalid={missing.code || pairing.error ? 'true' : undefined}
				aria-describedby="{id}-code-error"
			/>
			<p class="form-error" id="{id}-code-error">{missing.code || pairing.error}</p>
		</div>
		<div class="field">
			<label for="{id}-name">{m.common_name()}</label>
			<input
				id="{id}-name"
				bind:this={nameInput}
				bind:value={form.name}
				placeholder={m.shutters_name_placeholder()}
				aria-invalid={missing.name ? 'true' : undefined}
				aria-describedby="{id}-name-error"
			/>
			<p class="form-error" id="{id}-name-error">{missing.name}</p>
		</div>
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
