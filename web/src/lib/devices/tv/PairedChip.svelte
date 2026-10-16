<script lang="ts">
	// Which channel the box's keys take, on its row and its page: paired over Remote v2, the
	// difference between 8 ms and 150 ms a press. Nothing while they go over ADB.
	import { m } from '#lib/paraglide/messages.js';
	import Icon from '#lib/components/Icon.svelte';

	let { paired }: { paired: boolean | undefined } = $props();
</script>

{#if paired}<span class="chip accent"><Icon name="zap" size={12} />{m.android_tv_paired()}</span>{/if}
