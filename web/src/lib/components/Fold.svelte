<script lang="ts" module>
	import type { Snippet } from 'svelte';

	/** The button that unfolds it: its class, its name when its body has no words (a gear). */
	export interface FoldTrigger {
		class: string;
		label?: string;
		body: Snippet;
	}
	/** What unfolds: its class, and what it holds (rendered only while open). */
	export interface FoldPanel {
		class?: string;
		body: Snippet;
	}
</script>

<script lang="ts">
	// The one Bits UI Collapsible (APG disclosure: the button says whether it is open and
	// `aria-controls` what unfolds), whoever folds something: a label's Disclosure, a tile's
	// settings under its gear (DeviceTile `settings`). The caller places both parts (`children`
	// gets them as snippets: the button may sit in a row far from what it opens). What unfolds
	// exists only while open (a form starts afresh each time); closed while the focus was in it
	// (a form saved, a field cancelled), the focus goes back to its button (WCAG 2.4.3).
	import { Collapsible } from 'bits-ui';
	import { refocus } from '#lib/focus.ts';

	interface Props {
		open?: boolean;
		/** Gets the button and the panel to place. */
		children: Snippet<[trigger: Snippet<[FoldTrigger]>, panel: Snippet<[FoldPanel]>]>;
	}
	let { open = $bindable(false), children }: Props = $props();
	let button = $state<HTMLElement | null>(null);
	let content = $state<HTMLElement | null>(null);

	// before the DOM drops it: was the focus inside?
	$effect.pre(() => {
		if (!open && content?.contains(document.activeElement)) void refocus(() => button);
	});
</script>

{#snippet trigger({ class: cls, label, body }: FoldTrigger)}
	<Collapsible.Trigger class={cls} aria-label={label} bind:ref={button}>{@render body()}</Collapsible.Trigger>
{/snippet}

{#snippet panel({ class: cls, body }: FoldPanel)}
	<Collapsible.Content bind:ref={content}>
		{#snippet child({ props, open: shown })}
			<div {...props} class={cls}>
				{#if shown}{@render body()}{/if}
			</div>
		{/snippet}
	</Collapsible.Content>
{/snippet}

<Collapsible.Root bind:open>
	{#snippet child()}{@render children(trigger, panel)}{/snippet}
</Collapsible.Root>
