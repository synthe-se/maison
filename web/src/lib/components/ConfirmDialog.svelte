<script lang="ts">
	// A confirmation for what cannot be undone (docs/ux/tableau-de-bord.md § 6): the
	// consequence said, a button with the precise verb and « Garder », no default button: the
	// focus goes to the title.
	import { AlertDialog } from 'bits-ui';
	import { m } from '#lib/paraglide/messages.js';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';

	interface Props {
		/** The button that opens it. */
		label: string;
		icon?: IconName;
		title: string;
		description: string;
		/** The precise verb (« Remettre à zéro »). */
		action: string;
		onconfirm: () => void;
		pending?: boolean;
		disabled?: boolean;
		/** The trigger removes or resets something: in brick. */
		danger?: boolean;
	}
	let { label, icon, title, description, action, onconfirm, pending = false, disabled = false, danger = false }: Props = $props();
	let open = $state(false);
	let heading = $state<HTMLElement | null>(null);
</script>

<AlertDialog.Root bind:open>
	<AlertDialog.Trigger class={['btn', danger && 'danger']} disabled={disabled || pending} aria-busy={pending}>
		{#if pending}<Icon name="loader-circle" class="spin" />{:else if icon}<Icon name={icon} />{/if}{label}
	</AlertDialog.Trigger>
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
					<AlertDialog.Cancel class="btn">{m.device_keep()}</AlertDialog.Cancel>
					<AlertDialog.Action
						class="btn primary"
						onclick={() => {
							open = false;
							onconfirm();
						}}
					>{action}</AlertDialog.Action>
				</div>
			</div>
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>

<style>
	:global(.modal.confirm) { width: min(420px, calc(100vw - 32px)); }
	:global(.modal.confirm .btn) { min-height: var(--control-h); }
</style>
