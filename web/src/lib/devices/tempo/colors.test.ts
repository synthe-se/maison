import { describe, expect, it } from 'vitest';
import { TEMPO, TEMPO_COLORS, dayWords, probable } from './colors.ts';

describe('Tempo colours', () => {
	it('are listed cheapest first, each with a name and a shape (never the colour alone)', () => {
		expect(TEMPO_COLORS).toEqual(['BLUE', 'WHITE', 'RED']);
		expect(TEMPO_COLORS.map((c) => TEMPO[c].name())).toEqual(['Bleu', 'Blanc', 'Rouge']);
		expect(TEMPO_COLORS.map((c) => TEMPO[c].shape)).toEqual(['bleu', 'blanc', 'rouge']);
	});

	it('says a forecast with its probability (« Rouge probable · 62 % »)', () => {
		expect(probable('RED', 0.62)).toMatch(/^Rouge probable · 62\s%$/u);
		expect(probable('BLUE', undefined)).toBe('Bleu');
	});

	it('says a day as published, forecast or unknown', () => {
		expect(dayWords('RED', false, 0.62)).toBe('Rouge');
		expect(dayWords('WHITE', true, 0.4)).toMatch(/^Blanc probable · 40\s%$/u);
		expect(dayWords('BLUE', true)).toBe('Bleu');
		expect(dayWords(null, true, 0.9)).toBe('Inconnu');
	});
});
