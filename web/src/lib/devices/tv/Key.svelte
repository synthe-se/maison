<script lang="ts">
	// One key of a remote: sent on touch (not on release, so it feels immediate), repeated while
	// held when `every` > 0, shown pressed for at least 100 ms. A keyboard or screen-reader
	// activation (a click with no pointer) sends once.
	import type { Snippet } from 'svelte';
	import { haptic, TAP } from '#lib/haptics.ts';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import { holdRepeat, PAD_KEY, PRESSED_MIN, repeatEvery, type PadKey } from './remote.ts';

	interface Props {
		/** A pad key: its name, icon, feedback and repeat come from PAD_KEY. */
		k?: PadKey;
		label?: string;
		icon?: IconName;
		/** Sends the key; `repeat` is true for the steps of a hold (see `paced`). */
		fire: (repeat: boolean) => unknown;
		every?: number;
		/** key: 56 px, ok: 64 px (--remote-key, --remote-ok), small: 48 px (Back, Home, Menu). */
		size?: 'key' | 'ok' | 'small';
		pattern?: typeof TAP;
		disabled?: boolean;
		/** A pressed toggle (Ambilight): `aria-pressed`, the label stays the same. */
		pressed?: boolean;
		children?: Snippet;
	}
	let { k, fire, size = 'key', disabled = false, pressed, children, ...rest }: Props = $props();
	const label = $derived(rest.label ?? (k ? PAD_KEY[k].label() : ''));
	const icon = $derived(rest.icon ?? (k ? PAD_KEY[k].icon : undefined));
	const pattern = $derived(rest.pattern ?? (k ? PAD_KEY[k].pattern : TAP));
	const every = $derived(rest.every ?? (k ? repeatEvery(k) : 0));

	let down = $state(false);
	let stop = () => {};
	let downAt = 0;

	function start(e: PointerEvent) {
		if (disabled || e.button !== 0) return;
		(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
		down = true;
		downAt = Date.now();
		// feedback on touch, never on the answer (haptics.ts); nothing during the repeat (§ 7)
		haptic(pattern);
		void fire(false);
		stop = holdRepeat(fire, every);
	}

	function end() {
		stop();
		stop = () => {};
		if (!down) return;
		setTimeout(() => (down = false), Math.max(0, PRESSED_MIN - (Date.now() - downAt)));
	}

	$effect(() => () => stop());
</script>

<button
	type="button"
	class="key {size}"
	class:down
	aria-label={label}
	aria-pressed={pressed}
	title={label}
	{disabled}
	onpointerdown={start}
	onpointerup={end}
	onpointercancel={end}
	onlostpointercapture={end}
	oncontextmenu={(e) => e.preventDefault()}
	onclick={(e) => {
		if (e.detail === 0) {
			haptic(pattern);
			void fire(false);
		}
	}}
>
	{#if icon}<Icon name={icon} size={size === 'small' ? 18 : 22} />{/if}{@render children?.()}
</button>

<style>
	.key {
		flex: none; display: grid; place-items: center; width: var(--remote-key); height: var(--remote-key); padding: 0;
		border-radius: 50%; border: 1px solid var(--line); background: var(--surface); color: var(--ink); cursor: pointer;
		font: var(--t-label); touch-action: manipulation; user-select: none; -webkit-user-select: none; -webkit-touch-callout: none;
	}
	.key.ok { width: var(--remote-ok); height: var(--remote-ok); background: var(--ground-raised); }
	/* Back, Home, Menu: 48 px under the pad (docs/ux/tableau-de-bord.md § 7) */
	.key.small { width: var(--tile-icon); height: var(--tile-icon); }
	.key:hover:not([disabled]) { border-color: var(--accent-soft); }
	.key.down, .key:active:not([disabled]) { background: var(--accent-wash); border-color: var(--accent); transform: scale(0.96); }
	.key[aria-pressed='true'] { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
	.key[disabled] { opacity: 0.4; cursor: not-allowed; }
	.key:focus-visible { outline: 3px solid var(--accent); outline-offset: 2px; }
	@media (prefers-reduced-motion: reduce) { .key.down, .key:active { transform: none; } }
</style>
