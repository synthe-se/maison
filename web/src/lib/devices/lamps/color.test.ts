import { describe, expect, it, vi } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { EFFECTS, PRESETS, STOP_EFFECT, css, hsvToRgb, paintWheel, rgbToXy, wheelColor, wheelPosition, xyToRgb, type Rgb } from './color.ts';

const close = (a: number, b: number, d = 0.01) => Math.abs(a - b) <= d;

describe('colour maths', () => {
	it('white and black in xy', () => {
		const white = rgbToXy([255, 255, 255]);
		expect(close(white.x, 0.3227) && close(white.y, 0.329)).toBe(true);
		// black has no chromaticity: D65 white, not NaN
		expect(rgbToXy([0, 0, 0])).toEqual({ x: 0.3127, y: 0.329 });
	});

	it('xy → rgb → xy comes back to the same colour', () => {
		for (const rgb of [
			[255, 0, 0],
			[0, 255, 0],
			[0, 0, 255],
			[255, 128, 0]
		] as Rgb[]) {
			const { x, y } = rgbToXy(rgb);
			const back = rgbToXy(xyToRgb(x, y));
			expect(close(back.x, x, 0.02) && close(back.y, y, 0.02)).toBe(true);
		}
	});

	it('xyToRgb survives y = 0 and stays within 0–255', () => {
		const rgb = xyToRgb(0.7, 0);
		for (const c of rgb) expect(c >= 0 && c <= 255).toBe(true);
	});

	it('hsv covers the six sectors of the wheel', () => {
		expect(hsvToRgb(0, 1, 1)).toEqual([255, 0, 0]);
		expect(hsvToRgb(60, 1, 1)).toEqual([255, 255, 0]);
		expect(hsvToRgb(120, 1, 1)).toEqual([0, 255, 0]);
		expect(hsvToRgb(180, 1, 1)).toEqual([0, 255, 255]);
		expect(hsvToRgb(240, 1, 1)).toEqual([0, 0, 255]);
		expect(hsvToRgb(300, 1, 1)).toEqual([255, 0, 255]);
		expect(hsvToRgb(0, 0, 1)).toEqual([255, 255, 255]);
	});

	it('writes CSS colours', () => {
		expect(css([1, 2, 3])).toBe('rgb(1 2 3)');
	});
});

describe('the wheel', () => {
	it('its centre is white, its edge saturated; outside it there is no colour', () => {
		const centre = wheelColor(0.5, 0.5)!;
		expect(close(centre.x, 0.3227) && close(centre.y, 0.329)).toBe(true);
		expect(wheelColor(1, 0.5)).toEqual(rgbToXy([255, 0, 0]));
		expect(wheelColor(0, 0)).toBeNull();
	});

	it('places a colour where picking it there would give it back', () => {
		for (const [left, top] of [
			[1, 0.5],
			[0.5, 1],
			[0, 0.5],
			[0.5, 0],
			[0.7, 0.6]
		]) {
			const xy = wheelColor(left, top)!;
			const pos = wheelPosition(xy.x, xy.y);
			expect(close(pos.left, left, 0.03) && close(pos.top, top, 0.03)).toBe(true);
		}
	});

	it('white sits at the centre', () => {
		const { x, y } = rgbToXy([255, 255, 255]);
		const pos = wheelPosition(x, y);
		expect(close(pos.left, 0.5) && close(pos.top, 0.5)).toBe(true);
	});

	it('paints the disc, leaves the corners transparent', () => {
		const data = new Uint8ClampedArray(4 * 4 * 4);
		const ctx = { createImageData: vi.fn(() => ({ data })), putImageData: vi.fn() };
		const canvas = { width: 4, height: 4, getContext: () => ctx } as unknown as HTMLCanvasElement;
		paintWheel(canvas);
		expect(ctx.putImageData).toHaveBeenCalledOnce();
		expect(data[3]).toBe(0); // corner (0, 0)
		expect(data[(2 * 4 + 2) * 4 + 3]).toBe(255); // centre
	});

	it('does nothing without a 2D context', () => {
		const canvas = { width: 4, height: 4, getContext: () => null } as unknown as HTMLCanvasElement;
		expect(() => paintWheel(canvas)).not.toThrow();
	});
});

describe('named colours and effects', () => {
	it('each preset has a name a reader hears', () => {
		expect(PRESETS.map((p) => p.name())).toContain(m.color_red());
		expect(new Set(PRESETS.map((p) => p.name())).size).toBe(PRESETS.length);
	});

	it('effects carry the ids the server expects, and a way to stop', () => {
		expect(EFFECTS.map((e) => e.id)).toEqual(['candle', 'fireplace', 'colorloop', 'sunrise', 'sparkle', 'opal', 'glisten', 'blink', 'breathe', 'okay']);
		expect(EFFECTS.every((e) => e.name() !== '')).toBe(true);
		expect(STOP_EFFECT).toBe('stop_hue_effect');
	});
});
