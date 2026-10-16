<script lang="ts">
	// What « Tester » answered: passed or not, then each action with its own outcome, named with
	// the devices' names, and why it failed in the refusal's words (actions.ts `outcome`).
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';
	import type { KeyResponse } from './api.ts';
	import { outcome, type Sources } from './actions.ts';

	let { response, sources }: { response: KeyResponse; sources: Sources } = $props();
</script>

<div class="callout" class:warn={!response.success}>
	<p class="outcome">
		<Icon name={response.success ? 'check' : 'triangle-alert'} />
		{response.success ? m.remote_test_passed() : m.remote_test_failed()}
	</p>
	<ol class="results">
		{#each response.results as result, i (i)}
			{@const { ok, what, why } = outcome(result, sources)}
			<li>
				<span>{what}</span>
				<strong>{ok ? m.remote_test_item_ok() : m.remote_test_item_failed()}</strong>
				{#if why}<span class="detail">{why}</span>{/if}
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
	}
</style>
