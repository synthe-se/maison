<script lang="ts">
	// A Tempo day's mark: colour AND shape (docs/ux/tableau-de-bord.md § 8). Bleu filled, blanc an
	// empty ring, rouge filled and hatched at 45°; a forecast is a dashed outline over a wash;
	// unknown is a dashed muted ring. Always beside the colour's name: the mark itself is hidden
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

<span class="swatch {size} {color ? TEMPO[color].shape : 'none'}" class:forecast class:unsure class:current aria-hidden="true">
	{@render children?.()}
</span>

<style>
	.swatch {
		--c: var(--ink-muted);
		flex: none; display: inline-grid; place-items: center; align-content: center;
		border: 1.5px solid var(--c); color: var(--ink);
		font-variant-numeric: tabular-nums; line-height: 1;
	}
	.dot { width: 14px; height: 14px; border-radius: 50%; }
	.tile { width: var(--tile-icon); height: var(--tile-icon); border-radius: 50%; border-width: 2px; }
	.cell { width: 100%; aspect-ratio: 1; max-width: var(--tile-icon); border-radius: var(--radius-m); font: var(--t-meta); }

	.bleu { --c: var(--tempo-bleu); }
	.blanc { --c: var(--tempo-blanc-ring); }
	.rouge { --c: var(--tempo-rouge); }
	.none { border-style: dashed; }

	/* published: bleu and rouge filled (text on them in --on-tempo), blanc stays a ring.
	   The hatching mixes towards --ink, so it only ever adds contrast to the text on it. */
	.bleu:not(.forecast) { background: var(--tempo-bleu); color: var(--on-tempo); }
	.rouge:not(.forecast) {
		background: repeating-linear-gradient(45deg, var(--tempo-rouge) 0 3px, color-mix(in srgb, var(--tempo-rouge) 70%, var(--ink)) 3px 6px);
		color: var(--on-tempo);
	}
	/* --tempo-blanc: white inside its ring in light, nothing inside it in dark: the ink reads */
	.blanc:not(.forecast) { background: var(--tempo-blanc); color: var(--ink); }

	/* a forecast: dashed outline over a light wash of the colour, rouge still hatched */
	.forecast { border-style: dashed; border-width: 2px; }
	.bleu.forecast { background: color-mix(in srgb, var(--tempo-bleu) 18%, transparent); }
	.rouge.forecast {
		background: repeating-linear-gradient(45deg, color-mix(in srgb, var(--tempo-rouge) 35%, transparent) 0 3px, transparent 3px 6px);
	}

	/* not sure: dotted, and only the rouge keeps a faint hatching (it is still a shape) */
	.forecast.unsure { border-style: dotted; }
	.bleu.forecast.unsure { background: none; }
	.rouge.forecast.unsure {
		background: repeating-linear-gradient(45deg, color-mix(in srgb, var(--tempo-rouge) 18%, transparent) 0 3px, transparent 3px 6px);
	}

	.current { outline: 2px solid var(--accent); outline-offset: 2px; }
	.cell.current { font-weight: 700; }
</style>
