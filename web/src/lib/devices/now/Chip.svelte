<script lang="ts">
	// One chip of the « Maintenant » strip: a fact in words that leads to its group (`to`: the
	// group title's id); what needs a hand in the warning style and its triangle (the words say
	// it too). No icon otherwise: the strip stays on two lines on a phone.
	import Icon from '#lib/components/Icon.svelte';
	import { jump } from './jump.ts';

	let { text, to, warn = false }: { text: string; to: string; warn?: boolean } = $props();
</script>

<li>
	<button class="pill-btn now-chip" class:warn onclick={() => jump(to)}>
		{#if warn}<Icon name="triangle-alert" size={14} />{/if}{text}
	</button>
</li>
