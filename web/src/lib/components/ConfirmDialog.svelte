<script lang="ts">
	// A confirmation for what cannot be undone (docs/ux.md § 6): the
	// consequence said, a button with the precise verb and « Garder », no default button: the
	// focus goes to the title.
	import { AlertDialog } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import { pending } from '#lib/gesture.svelte.ts';

	interface Props {
		/** The button that opens it; without one, the caller opens it (`open`). */
		label?: string;
		open?: boolean;
		/** The way back, when « Garder » is not the word (« Continuer »). */
		keep?: string;
		/** Its name for readers when the label alone is ambiguous (« Retirer la clé MacBook »). */
		ariaLabel?: string;
		/** A quiet trigger, in a row of actions. */
		ghost?: boolean;
		icon?: IconName;
		title: string;
		description: string;
		/** The precise verb (« Remettre à zéro »). */
		action: string;
		onconfirm: () => void;
		/** Its gesture travels: busy, it keeps the focus and does not open again. */
		busy?: boolean;
		/** The trigger removes or resets something: in brick. */
		danger?: boolean;
	}
	let {
		label,
		open = $bindable(false),
		keep,
		ariaLabel,
		ghost = false,
		icon,
		title,
		description,
		action,
		onconfirm,
		busy = false,
		danger = false
	}: Props = $props();
	let heading = $state<HTMLElement | null>(null);
</script>

<AlertDialog.Root bind:open>
	<!-- busy, it keeps the focus (never disabled under the finger) and does not open again -->
	{#if label}<AlertDialog.Trigger
			class={['btn', danger && 'danger', ghost && 'ghost']}
			aria-label={ariaLabel}
			{...pending(busy)}
			onclick={(e: MouseEvent) => busy && e.preventDefault()}
		>
			{#if busy || icon}<Icon name={icon ?? 'loader-circle'} {busy} />{/if}{label}
		</AlertDialog.Trigger>{/if}
	<AlertDialog.Portal>
		<AlertDialog.Overlay class="overlay" />
		<AlertDialog.Content
			class="modal confirm"
			onOpenAutoFocus={(e) => {
				e.preventDefault();
				heading?.focus();
			}}
		>
			<div class="dlg-body">
				<AlertDialog.Title level={2} tabindex={-1} bind:ref={heading} class="group-title">{title}</AlertDialog.Title>
				<AlertDialog.Description class="hint">{description}</AlertDialog.Description>
				<div class="actions end">
					<AlertDialog.Cancel class="btn">{keep ?? m.device_keep()}</AlertDialog.Cancel>
					<AlertDialog.Action
						class="btn primary"
						onclick={() => {
							open = false;
							onconfirm();
						}}>{action}</AlertDialog.Action
					>
				</div>
			</div>
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>

<style>
	:global(.modal.confirm) {
		width: min(var(--confirm-w), calc(100vw - var(--s-6)));
	}
	:global(.modal.confirm .btn) {
		min-height: var(--control-h);
	}
</style>
