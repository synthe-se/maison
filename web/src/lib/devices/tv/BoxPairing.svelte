<script lang="ts">
	// Pairing the box over Remote v2 (an admin's, in its settings): optional, but it is what
	// makes keys fast (~8 ms over Remote v2, ~150 ms over ADB). Two calls with a person reading
	// a code off the TV in between; a code refused is said under the field, which keeps the focus.
	import { tick } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import { androidTvApi } from './api.ts';

	/** Paired: the box is read again (its settings close, the focus goes back to their gear). */
	let { onpaired }: { onpaired: () => unknown } = $props();
	const id = $props.id();

	/** The code the TV shows: six characters. */
	const CODE_LENGTH = 6;
	const pairing = new Gesture();
	let open = $state(false);
	let code = $state('');
	let input = $state<HTMLInputElement>();
	/** The code typed is not six characters: said under the field. */
	let problem = $state('');

	function start() {
		return pairing.run(
			() => androidTvApi.pairStart(),
			async () => {
				open = true;
				ui.say(m.android_tv_pair_started());
				await tick();
				input?.focus();
			}
		);
	}

	function finish(e: SubmitEvent) {
		e.preventDefault();
		const typed = code.trim();
		problem = typed.length === CODE_LENGTH ? '' : m.android_tv_pair_code_length({ count: CODE_LENGTH });
		if (problem) return void input?.focus();
		return pairing.run(
			() => androidTvApi.pairFinish(typed),
			async () => {
				haptic(CONFIRM);
				ui.toast(m.android_tv_paired_ok());
				// the settings close while the focus is still in the form: it goes back to their gear
				await onpaired();
				open = false;
				code = '';
			},
			'finish',
			{ field: () => input }
		);
	}
</script>

<div class="block">
	<p class="hint">{m.android_tv_pair_hint()}</p>
	{#if open}
		<form class="pair" onsubmit={finish} novalidate>
			<div class="actions">
				<div class="field">
					<label for="{id}-code">{m.android_tv_pair_code()}</label>
					<input
						id="{id}-code"
						bind:this={input}
						bind:value={code}
						oninput={() => (problem = '')}
						maxlength={CODE_LENGTH}
						autocomplete="one-time-code"
						autocapitalize="characters"
						spellcheck="false"
						aria-invalid={problem || pairing.error ? 'true' : undefined}
						aria-describedby="{id}-error"
					/>
				</div>
				<button class="btn primary" {...pending(pairing.is('finish'))}>
					<Icon name="check" busy={pairing.is('finish')} />{m.android_tv_pair_confirm()}
				</button>
			</div>
			<p class="form-error" id="{id}-error">{problem || pairing.error}</p>
		</form>
	{:else}
		<div class="actions">
			<!-- a stable button: its label says the wait, the focus stays on it -->
			<button class="btn" {...pending(pairing.is())} onclick={start}>
				<Icon name="zap" busy={pairing.is()} />{pairing.is() ? m.android_tv_pairing() : m.android_tv_pair()}
			</button>
		</div>
	{/if}
</div>

<style>
	.pair .actions {
		align-items: end;
	}
	.pair input {
		width: 9ch;
		font-family: var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.2em;
	}
</style>
