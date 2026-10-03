// The Tempo colours, once (docs/ux/tableau-de-bord.md § 8): each colour has a name, a token and
// a shape, never the colour alone. Bleu: filled; blanc: an empty ring; rouge: filled and
// hatched at 45°. The shapes are drawn by Swatch.svelte from `shape`; every view (the tile,
// the calendar, the forecasts, the Nabaztag) imports this module rather than its own map.

import { m } from '#lib/paraglide/messages.js';
import { percent } from '#lib/i18n.svelte.ts';
import type { TempoColor } from '#lib/api.ts';

export type { TempoColor };

/** In the order of the tariff, cheapest first: legends, probabilities and tariff tables. */
export const TEMPO_COLORS: readonly TempoColor[] = ['BLUE', 'WHITE', 'RED'];

/** `key`: the colour's field in the server's per-colour objects (prices, days left). */
export const TEMPO: Record<TempoColor, { name: () => string; token: string; shape: 'bleu' | 'blanc' | 'rouge'; key: 'blue' | 'white' | 'red' }> = {
	BLUE: { name: m.color_blue, token: 'var(--tempo-bleu)', shape: 'bleu', key: 'blue' },
	WHITE: { name: m.color_white, token: 'var(--tempo-blanc-ring)', shape: 'blanc', key: 'white' },
	RED: { name: m.color_red, token: 'var(--tempo-rouge)', shape: 'rouge', key: 'red' }
};

/** Under this probability a forecast is drawn « not sure » (a dotted outline, no wash). */
export const UNSURE = 0.6;

/** A forecast in words, with its probability (« Rouge probable · 62 % »). */
export function probable(c: TempoColor, p: number | undefined): string {
	return p === undefined ? TEMPO[c].name() : m.tempo_color_probable({ color: TEMPO[c].name(), percent: percent(p) });
}

/** A day as said: published (« Rouge »), forecast (« Rouge probable · 62 % ») or unknown. */
export function dayWords(color: TempoColor | null | undefined, forecast: boolean, p?: number): string {
	if (!color) return m.common_unknown();
	return forecast ? probable(color, p) : TEMPO[color].name();
}
