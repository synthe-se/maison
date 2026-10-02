<script lang="ts">
	// A share of something drawn as a bar in a Tempo colour, beside the same figure in words
	// (a probability, the stock used): decorative, hidden from assistive technology.
	import { TEMPO, type TempoColor } from './colors.ts';

	let { color, share }: { color: TempoColor; share: number } = $props();
</script>

<span class="bar" aria-hidden="true"><span style:width="{Math.min(1, Math.max(0, share)) * 100}%" style:background={TEMPO[color].token}></span></span>

<style>
	.bar { display: block; height: 8px; border-radius: var(--radius-pill); background: var(--ground-raised); overflow: hidden; }
	.bar span { display: block; height: 100%; border-radius: var(--radius-pill); }
</style>
