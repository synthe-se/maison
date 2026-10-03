<script lang="ts">
	// What « Tester » answered: passed or not, then each action with its own outcome and what
	// the backend said (« ok: … » / « failed: … », read by actions.ts `outcome`).
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import type { IrAction, IrTestResponse } from './api.ts';
	import { outcome, summarize, type Sources } from './actions.ts';

	let { response, actions, sources }: { response: IrTestResponse; actions: IrAction[]; sources: Sources } = $props();
</script>

<div class="callout" class:warn={!response.success}>
	<p class="outcome">
		<Icon name={response.success ? 'check' : 'triangle-alert'} />
		{response.success ? m.remote_test_passed() : m.remote_test_failed()}
	</p>
	<ol class="results">
		{#each response.results as result, i (i)}
			{@const { ok, detail } = outcome(result)}
			<li>
				<span>{actions[i] ? summarize(actions[i], sources) : ''}</span>
				<strong>{ok ? m.remote_test_item_ok() : m.remote_test_item_failed()}</strong>
				<span class="detail">{detail}</span>
			</li>
		{/each}
	</ol>
</div>

<style>
	.outcome {
		display: flex;
		align-items: center;
		gap: var(--s-2);
		font-weight: 600;
	}
	.results {
		margin: 0;
		padding-left: var(--s-5);
		display: grid;
		gap: var(--s-2);
		font: var(--t-secondary);
	}
	.results li > * {
		display: block;
	}
	.detail {
		color: var(--ink-muted);
		font-family: var(--font-mono);
		overflow-wrap: anywhere;
	}
</style>
