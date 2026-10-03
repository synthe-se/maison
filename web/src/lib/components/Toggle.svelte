<script lang="ts">
	// On/off for something the house confirms (APG switch): the state is the device's, never
	// guessed; while the command travels the switch says so and does not flip twice. Busy, it
	// keeps the focus (never `disabled`: the focus would fall to the page, WCAG 2.4.3) and
	// ignores presses (docs/ux.md § 2). One that cannot act stays in place, not operable, and
	// says why (`reason`: the id of the words, `unavailable`).
	import { Switch } from 'bits-ui';
	import { pending, unavailable } from '#lib/gesture.svelte.ts';

	interface Props {
		label: string;
		checked: boolean;
		onchange: (next: boolean) => void;
		/** Its order travels: busy, it keeps the focus and ignores presses. */
		busy?: boolean;
		/** The id of what says why it cannot act now; none: it can. */
		reason?: string;
		hideLabel?: boolean;
	}
	let { label, checked, onchange, busy = false, reason, hideLabel = false }: Props = $props();
	const id = $props.id();
</script>

<div class="toggle">
	<label for={id} class:sr-only={hideLabel}>{label}</label>
	<!-- controlled: the switch shows what the device says, a press only asks -->
	<Switch.Root
		{id}
		bind:checked={() => checked, (v) => !busy && !reason && onchange(v)}
		{...pending(busy)}
		{...unavailable(reason)}
		class="switch"
	>
		<Switch.Thumb class="switch-thumb" />
	</Switch.Root>
</div>

<style>
	.toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--s-3);
		min-height: var(--control-h);
	}
	.toggle label {
		font: var(--t-label);
	}
	:global(.switch) {
		position: relative;
		width: var(--switch-w);
		height: var(--switch-h);
		padding: var(--s-half);
		border-radius: var(--radius-pill);
		cursor: pointer;
		border: 2px solid var(--ink-muted);
		background: var(--ground-raised);
		display: inline-flex;
		align-items: center;
	}
	:global(.switch[data-state='checked']) {
		background: var(--accent);
		border-color: var(--accent);
	}
	:global(.switch[aria-busy='true']) {
		cursor: progress;
		opacity: var(--disabled-opacity);
	}
	:global(.switch[aria-disabled='true']:not([aria-busy='true'])) {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}
	/* the thumb moves and the track fills: on/off read by shape and place, not color alone */
	:global(.switch-thumb) {
		display: block;
		width: var(--switch-thumb);
		height: var(--switch-thumb);
		border-radius: 50%;
		background: var(--ink-muted);
		transition: transform 0.15s ease-out;
	}
	:global(.switch[data-state='checked'] .switch-thumb) {
		transform: translateX(var(--switch-thumb));
		background: var(--on-accent);
	}
</style>
