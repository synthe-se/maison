<script lang="ts">
	// The TV whose state is assumed (docs/ux.md § 2, case 3: powered up, JointSPACE silent): two
	// buttons, « Allumer » and « Éteindre », each busy while its order travels; the same on its
	// dashboard row and on its page.
	import { m } from '#lib/paraglide/messages.js';
	import { pending } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import type { TvRemote } from './tv.svelte.ts';

	let { tv }: { tv: TvRemote } = $props();
</script>

<div class="btn-row">
	<button class="btn" {...pending(tv.g.is('on'))} onclick={() => tv.assumedPower(true)}>
		<Icon name="power" busy={tv.g.is('on')} /><span class="btn-text">{m.action_turn_on()}</span>
	</button>
	<button class="btn" {...pending(tv.g.is('off'))} onclick={() => tv.assumedPower(false)}>
		<Icon name="power" busy={tv.g.is('off')} /><span class="btn-text">{m.action_turn_off()}</span>
	</button>
</div>
