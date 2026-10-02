// The Tempo colours, once (docs/ux/tableau-de-bord.md § 8): each colour has a name, a token and
// a shape, never the colour alone. Bleu: filled; blanc: an empty ring; rouge: filled and
// hatched at 45°. The shapes are drawn by Swatch.svelte from `shape`; every view (the tile,
// the calendar, the forecasts, the Nabaztag) imports this module rather than its own map.

import { m } from '#lib/paraglide/messages.js';
import { percent } from '#lib/i18n.svelte.ts';

export type TempoColor = 'BLUE' | 'WHITE' | 'RED';

/** In the order of the tariff, cheapest first: legends, probabilities and tariff tables. */
export const TEMPO_COLORS: readonly TempoColor[] = ['BLUE', 'WHITE', 'RED'];

export const TEMPO: Record<TempoColor, { name: () => string; token: string; shape: 'bleu' | 'blanc' | 'rouge'; tarif: 'blue' | 'white' | 'red' }> = {
	BLUE: { name: m.color_blue, token: 'var(--tempo-bleu)', shape: 'bleu', tarif: 'blue' },
	WHITE: { name: m.color_white, token: 'var(--tempo-blanc-ring)', shape: 'blanc', tarif: 'white' },
	RED: { name: m.color_red, token: 'var(--tempo-rouge)', shape: 'rouge', tarif: 'red' }
};

/** « Rouge », or « Inconnu » when RTE has said nothing and nothing is forecast. */
export function colorName(c: TempoColor | null | undefined): string {
	return c ? TEMPO[c].name() : m.common_unknown();
}

/** A forecast in words, with its probability (« Rouge probable, 62 % »). */
export function probable(c: TempoColor, p: number | undefined): string {
	return p === undefined ? TEMPO[c].name() : m.tempo_color_probable({ color: TEMPO[c].name(), percent: percent(p) });
}

/** A day as said: published (« Rouge »), forecast (« Rouge probable, 62 % ») or unknown. */
export function dayWords(color: TempoColor | null | undefined, forecast: boolean, p?: number): string {
	if (!color) return m.common_unknown();
	return forecast ? probable(color, p) : TEMPO[color].name();
}
