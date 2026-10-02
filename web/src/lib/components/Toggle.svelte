<script lang="ts">
	// On/off for something the house confirms (APG switch): the state is the device's, never
	// guessed; while the command travels the switch says so and does not flip twice.
	import { Switch } from 'bits-ui';

	interface Props {
		label: string;
		checked: boolean;
		onchange: (next: boolean) => void;
		pending?: boolean;
		disabled?: boolean;
		hideLabel?: boolean;
	}
	let { label, checked, onchange, pending = false, disabled = false, hideLabel = false }: Props = $props();
	const id = $props.id();
</script>

<div class="toggle">
	<label for={id} class:sr-only={hideLabel}>{label}</label>
	<Switch.Root
		{id}
		{checked}
		disabled={disabled || pending}
		aria-busy={pending}
		onCheckedChange={(v) => onchange(v)}
		class="switch"
	>
		<Switch.Thumb class="switch-thumb" />
	</Switch.Root>
</div>

<style>
	.toggle { display: inline-flex; align-items: center; gap: var(--s-3); min-height: var(--control-h); }
	.toggle label { font: var(--t-label); }
	:global(.switch) {
		position: relative; width: 48px; height: 28px; padding: 2px; border-radius: var(--radius-pill); cursor: pointer;
		border: 2px solid var(--ink-muted); background: var(--ground-raised); display: inline-flex; align-items: center;
	}
	:global(.switch[data-state='checked']) { background: var(--accent); border-color: var(--accent); }
	:global(.switch[aria-busy='true']) { cursor: progress; opacity: 0.7; }
	:global(.switch[data-disabled]:not([aria-busy='true'])) { opacity: 0.55; cursor: not-allowed; }
	/* the thumb moves and the track fills: on/off read by shape and place, not colour alone */
	:global(.switch-thumb) { display: block; width: 20px; height: 20px; border-radius: 50%; background: var(--ink-muted); transition: transform 0.15s ease-out; }
	:global(.switch[data-state='checked'] .switch-thumb) { transform: translateX(20px); background: var(--on-accent); }
</style>
