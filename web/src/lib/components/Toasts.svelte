<script lang="ts">
	// The toasts, above the bottom bar (Material snackbar). A toast may carry one action
	// (« Rétablir »): its button is `toast-action-<id>`, for the caller to give it the focus;
	// when a toast holding the focus leaves (its action pressed, closed, or its time up), the
	// focus goes where its action says (`back`), never to the page.
	import { m } from '#lib/paraglide/messages.js';
	import { ui, type ToastAction } from '#lib/ui.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import Icon from './Icon.svelte';

	/** Removes a toast; the focus it held goes back where its action says. */
	function leave(id: number, action: ToastAction | undefined, el: HTMLElement | null) {
		const held = !!el?.contains(document.activeElement);
		ui.dismiss(id);
		if (held && action?.back) void refocus(action.back);
	}
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
			{#if t.action}
				{@const action = t.action}
				<button
					class="btn"
					id="toast-action-{t.id}"
					onclick={(e) => {
						const el = e.currentTarget.closest<HTMLElement>('.toast');
						action.run();
						leave(t.id, action, el);
					}}>{action.label}</button
				>
			{/if}
			<button class="toast-close" aria-label={m.dismiss()} onclick={(e) => leave(t.id, t.action, e.currentTarget.closest('.toast'))}>
				<Icon name="x" />
			</button>
		</div>
	{/each}
</div>
