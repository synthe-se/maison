<script lang="ts">
	// What only an admin may do (configure, pair, install, remove): shown to admins; a member
	// sees the short reason instead, or nothing when `reason` is false (the server refuses them
	// anyway: 403 `forbidden`).
	import type { Snippet } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { session } from '#lib/session.svelte.ts';
	import Icon from './Icon.svelte';

	let { reason, children }: { reason?: string | false; children: Snippet } = $props();
</script>

{#if session.admin}
	{@render children()}
{:else if reason !== false}
	<p class="hint admin-only"><Icon name="lock" size={14} />{reason ?? m.admin_only()}</p>
{/if}

<style>
	.admin-only { display: flex; align-items: center; gap: var(--s-2); }
</style>
