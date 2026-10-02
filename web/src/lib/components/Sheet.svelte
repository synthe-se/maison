<script lang="ts">
	// A form that needs room (a meal, a remote binding): a Bits UI Dialog drawn as a sheet, from
	// the bottom on a phone, from the right on a wide screen (Material 3; app.css `.sheet`).
	// Its title names what is edited; Escape, the overlay or the cross close it.
	import type { Snippet } from 'svelte';
	import { Dialog } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import Icon from './Icon.svelte';

	interface Props {
		open: boolean;
		onclose: () => void;
		title: string;
		description?: string;
		children: Snippet;
	}
	let { open, onclose, title, description, children }: Props = $props();
</script>

<Dialog.Root {open} onOpenChange={(next) => !next && onclose()}>
	<Dialog.Portal>
		<Dialog.Overlay class="overlay" />
		<Dialog.Content class="sheet">
			<div class="handle" aria-hidden="true"></div>
			<div class="dlg-head">
				<div class="grow">
					<Dialog.Title level={2} class="sheet-title">{title}</Dialog.Title>
					{#if description}<Dialog.Description class="hint">{description}</Dialog.Description>{/if}
				</div>
				<Dialog.Close class="icon-btn" aria-label={m.dismiss()}><Icon name="x" /></Dialog.Close>
			</div>
			<div class="sheet-body">
				{#if open}{@render children()}{/if}
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
