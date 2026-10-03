<script lang="ts">
	// The one slider (APG slider, through Bits UI; docs/ux.md § 3). The value is
	// spoken in words (`valueText`), Page Up / Page Down move `page` steps, the 44 px band can be
	// touched anywhere. Two uses:
	// - a form value: `oncommit` on release (or on each key press);
	// - a device: `send` on release, and with `live` also while dragging, at most every
	//   EVERY ms (a lamp, a volume); the thumb keeps the sent value until the device reports it (± `near`) or
	//   `hold` ms pass, so a slow device never makes it jump back; `settle` reads the device
	//   again after the gesture. Without `live` (a shutter's motor), keys do not send one
	//   target each: the last one goes KEYS_SETTLE ms after the last key (§ 3).
	// `send` is the device call itself (it rejects on failure): the slider sends through a gesture
	// of its own, so an order in flight elsewhere on the tile (Open, Close) never swallows a
	// position, and a failure is said the one way (Gesture) and the thumb goes back.
	// One that cannot act stays in place, not operable, and says why (`reason`).
	import { Slider } from 'bits-ui';
	import { Gesture, unavailable } from '#lib/gesture.svelte.ts';

	/** How long the thumb trusts the sent value over a device that has not caught up. */
	const HOLD = 3_000;
	/** While dragging a `live` slider, at most one send per this many ms (doc § 3). */
	const EVERY = 300;
	/** A device sent on release only gets its target this long after the last key (§ 3). */
	const KEYS_SETTLE = 400;

	interface Props {
		label: string;
		value: number;
		min?: number;
		max?: number;
		step?: number;
		/** Page Up / Page Down move this many steps (APG: a larger step). */
		page?: number;
		/** What the value means, spoken and shown: « Ouvert à 40 % ». */
		valueText: (v: number) => string;
		oncommit?: (v: number) => void;
		send?: (v: number) => Promise<unknown>;
		/** Send while dragging too (throttled), not only on release. */
		live?: boolean;
		settle?: () => Promise<unknown>;
		/** A reported value this close to the sent one counts as reached. */
		near?: number;
		/** Track-end labels (« Fermé » … « Ouvert »). */
		ends?: [string, string];
		/** The value the device reports while it moves to the thumb's (a fine mark on the track). */
		mark?: number;
		/** The id of what says why it cannot act now; none: it can. */
		reason?: string;
		/** Hide the visible label when the card already says it; it stays for readers. */
		hideLabel?: boolean;
	}
	let {
		label,
		value,
		min = 0,
		max = 100,
		step = 1,
		page = 10,
		valueText,
		oncommit,
		send,
		live = false,
		settle,
		near = step,
		ends,
		mark,
		reason,
		hideLabel = false
	}: Props = $props();
	const g = new Gesture();
	/** Each send its own key: a live slider may have two in flight. */
	let sends = 0;

	let held = $state<number | null>(null);
	const shown = $derived(held !== null && Math.abs(value - held) > near ? held : value);
	// a local value while dragging; the device's value wins again when it changes
	let draft = $derived(shown);
	const id = $props.id();

	let gate: ReturnType<typeof setTimeout> | undefined;
	let release: ReturnType<typeof setTimeout> | undefined;
	let queued: { v: number; final: boolean } | null = null;
	let lastSent: number | null = null;
	/** The gesture comes from the keyboard (until a pointer takes over). */
	let keyed = false;
	let settling: ReturnType<typeof setTimeout> | undefined;
	$effect(() => () => {
		clearTimeout(gate);
		clearTimeout(release);
		clearTimeout(settling);
	});

	function push(v: number, final: boolean) {
		if (!send) {
			if (final) oncommit?.(v);
			return;
		}
		if (!final && !live) return;
		if (final && !live && keyed) {
			held = v;
			clearTimeout(settling);
			settling = setTimeout(() => {
				keyed = false;
				push(v, true);
			}, KEYS_SETTLE);
			return;
		}
		held = v;
		clearTimeout(release);
		if (gate) queued = { v, final };
		else void fire(v, final);
	}

	async function fire(v: number, final: boolean) {
		if (live) {
			gate = setTimeout(() => {
				gate = undefined;
				const next = queued;
				queued = null;
				if (next) void fire(next.v, next.final);
			}, EVERY);
		}
		let failed = false;
		if (v !== lastSent) {
			lastSent = v;
			await g.run(() => send!(v), undefined, `send-${++sends}`, {
				// said by the gesture; the thumb goes back to what the device says
				refused: () => {
					failed = true;
					held = null;
					lastSent = null;
				}
			});
		}
		if (final && !failed) {
			await settle?.();
			release = setTimeout(() => (held = null), HOLD);
		}
	}

	function onkeydown(e: KeyboardEvent) {
		keyed = true;
		const dir = e.key === 'PageUp' ? 1 : e.key === 'PageDown' ? -1 : 0;
		if (!dir || reason) return;
		e.preventDefault();
		draft = Math.min(max, Math.max(min, draft + dir * page * step));
		push(draft, true);
	}
</script>

<div class="range">
	<div class="head" class:sr-only={hideLabel}>
		<span id="{id}-label">{label}</span>
		<output for="{id}-thumb" class="value">{valueText(draft)}</output>
	</div>
	<Slider.Root
		type="single"
		bind:value={() => draft, (v) => !reason && (draft = v)}
		{min}
		{max}
		{step}
		onValueChange={(v) => !reason && push(v, false)}
		onValueCommit={(v) => !reason && push(v, true)}
		onpointerdown={() => (keyed = false)}
		class="range-band"
	>
		<span class="track">
			<Slider.Range class="range-fill" />
			{#if mark !== undefined && Math.abs(mark - draft) > near}
				<span class="mark" style:left="{((mark - min) / (max - min)) * 100}%" aria-hidden="true"></span>
			{/if}
		</span>
		<!-- the thumb drawn here, so `unavailable` has the last word over Bits' aria-disabled
		     (Bits' own disabled would take it out of the Tab order: the focus would fall) -->
		<Slider.Thumb index={0} id="{id}-thumb" aria-labelledby="{id}-label" aria-valuetext={valueText(draft)} {onkeydown}>
			{#snippet child({ props })}
				<span {...props} class="range-thumb" {...unavailable(reason)}></span>
			{/snippet}
		</Slider.Thumb>
	</Slider.Root>
	{#if ends}
		<div class="ends" aria-hidden="true"><span>{ends[0]}</span><span>{ends[1]}</span></div>
	{/if}
</div>

<style>
	.range {
		display: grid;
		gap: var(--s-2);
	}
	.head,
	.ends {
		display: flex;
		justify-content: space-between;
		gap: var(--s-3);
		font: var(--t-secondary);
		color: var(--ink-muted);
	}
	.ends {
		font: var(--t-meta);
	}
	.value {
		color: var(--ink);
		font-variant-numeric: tabular-nums;
	}
	/* a 44 px band to grab (touching it places the thumb, WCAG 2.5.7), a thin track inside */
	.range :global(.range-band) {
		position: relative;
		display: flex;
		align-items: center;
		min-height: var(--control-h);
		user-select: none;
	}
	.track {
		position: relative;
		flex: 1;
		height: var(--track-h);
		border-radius: var(--radius-pill);
		background: var(--ground-raised);
		overflow: hidden;
	}
	.range :global(.range-fill) {
		position: absolute;
		height: 100%;
		background: var(--accent);
	}
	/* where the device is while it travels to the thumb */
	.mark {
		position: absolute;
		top: 0;
		bottom: 0;
		width: var(--s-half);
		margin-left: calc(-1 * var(--s-half) / 2);
		background: var(--ink);
	}
	.range :global(.range-thumb) {
		display: block;
		width: var(--thumb);
		height: var(--thumb);
		border-radius: 50%;
		background: var(--surface);
		border: 2px solid var(--accent);
		box-shadow: var(--shadow);
		cursor: grab;
	}
	.range :global(.range-thumb:focus-visible) {
		outline: var(--focus-ring);
		outline-offset: var(--focus-offset);
	}
	/* unreachable or off: the last known value, greyed */
	.range:has(:global([aria-disabled='true'])) .value {
		color: var(--ink-muted);
	}
	.range :global(.range-band:has([aria-disabled='true'])) {
		opacity: var(--disabled-opacity);
	}
</style>
