<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import Icon from './Icon.svelte';
</script>

<!-- not a live region: the layout's two regions say each outcome once -->
<div class="toasts">
	{#each ui.toasts as t (t.id)}
		<!-- svelte-ignore a11y_no_static_element_interactions: hover only pauses the timer -->
		<div
			class="toast"
			class:warn={t.warn}
			onmouseenter={() => ui.pause(t.id)}
			onmouseleave={() => ui.resume(t.id)}
			onfocusin={() => ui.pause(t.id)}
			onfocusout={() => ui.resume(t.id)}
		>
			<p>{t.text}</p>
			<button class="toast-close" aria-label={m.dismiss()} onclick={() => ui.dismiss(t.id)}>
				<Icon name="x" />
			</button>
		</div>
	{/each}
</div>
