<script lang="ts">
	// A Tempo day's mark: color AND shape (docs/ux.md § 8). Blue filled, white an
	// empty ring, red filled and hatched at 45°; a forecast is a dashed outline over a wash;
	// unknown is a dashed muted ring. Always beside the color's name: the mark itself is hidden
	// from assistive technology. A forecast under 60 % is « not sure »: dotted, no wash.
	import type { Snippet } from 'svelte';
	import { TEMPO, type TempoColor } from './colors.ts';

	interface Props {
		color: TempoColor | null | undefined;
		/** Not yet published by RTE: a forecast. */
		forecast?: boolean;
		/** A forecast under `UNSURE`. */
		unsure?: boolean;
		/** dot: beside a word; tile: the tile's 48 px glyph; cell: a calendar day. */
		size?: 'dot' | 'tile' | 'cell';
		/** Today, in the calendar: an accent outline. */
		current?: boolean;
		children?: Snippet;
	}
	let { color, forecast = false, unsure = false, size = 'dot', current = false, children }: Props = $props();
</script>

<span class="swatch {size} {color ? TEMPO[color].key : 'none'}" class:forecast class:unsure class:current aria-hidden="true">
	{@render children?.()}
</span>

<style>
	.swatch {
		--c: var(--ink-muted);
		flex: none;
		display: inline-grid;
		place-items: center;
		align-content: center;
		border: 1.5px solid var(--c);
		color: var(--ink);
		font-variant-numeric: tabular-nums;
		line-height: 1;
	}
	.dot {
		width: var(--dot);
		height: var(--dot);
		border-radius: 50%;
	}
	.tile {
		width: var(--tile-icon);
		height: var(--tile-icon);
		border-radius: 50%;
		border-width: 2px;
	}
	.cell {
		width: 100%;
		aspect-ratio: 1;
		max-width: var(--tile-icon);
		border-radius: var(--radius-m);
		font: var(--t-meta);
	}

	.blue {
		--c: var(--tempo-blue);
	}
	.white {
		--c: var(--tempo-white-ring);
	}
	.red {
		--c: var(--tempo-red);
	}
	.none {
		border-style: dashed;
	}

	/* published: blue and red filled (text on them in --on-tempo), white stays a ring.
	   The hatching mixes towards --ink, so it only ever adds contrast to the text on it. */
	.blue:not(.forecast) {
		background: var(--tempo-blue);
		color: var(--on-tempo);
	}
	.red:not(.forecast) {
		background: var(--tempo-red-hatch);
		color: var(--on-tempo);
	}
	/* --tempo-white: white inside its ring in light, nothing inside it in dark: the ink reads */
	.white:not(.forecast) {
		background: var(--tempo-white);
		color: var(--ink);
	}

	/* a forecast: dashed outline over a light wash of the color, red still hatched */
	.forecast {
		border-style: dashed;
		border-width: 2px;
	}
	.blue.forecast {
		background: color-mix(in srgb, var(--tempo-blue) 18%, transparent);
	}
	.red.forecast {
		background: repeating-linear-gradient(45deg, color-mix(in srgb, var(--tempo-red) 35%, transparent) 0 3px, transparent 3px 6px);
	}

	/* not sure: dotted, and only the red keeps a faint hatching (it is still a shape) */
	.forecast.unsure {
		border-style: dotted;
	}
	.blue.forecast.unsure {
		background: none;
	}
	.red.forecast.unsure {
		background: repeating-linear-gradient(45deg, color-mix(in srgb, var(--tempo-red) 18%, transparent) 0 3px, transparent 3px 6px);
	}

	.current {
		outline: var(--mark-ring);
		outline-offset: var(--focus-offset);
	}
	.cell.current {
		font-weight: 700;
	}
</style>
