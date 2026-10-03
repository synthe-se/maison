// Color maths for the Zigbee lamps: the lamps speak CIE xy (0–1), the screen speaks sRGB.
// The picker is a hue/saturation wheel; presets are named colors computed once.

import { m } from '#lib/paraglide/messages.js';

export type Rgb = [number, number, number];

const gamma = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055);
const linear = (c: number) => (c > 0.04045 ? Math.pow((c + 0.055) / 1.055, 2.4) : c / 12.92);

export function xyToRgb(x: number, y: number): Rgb {
	const safeY = Math.max(y, 0.00001);
	const Y = 1;
	const X = (Y / safeY) * x;
	const Z = (Y / safeY) * (1 - x - safeY);
	// wide-gamut to sRGB
	const raw = [
		X * 1.656492 - Y * 0.354851 - Z * 0.255038,
		-X * 0.707196 + Y * 1.655397 + Z * 0.036152,
		X * 0.051713 - Y * 0.121364 + Z * 1.01153
	];
	const max = Math.max(...raw, 1);
	return raw.map((c) => Math.round(gamma(Math.max(0, c / max)) * 255)) as Rgb;
}

export function rgbToXy([r, g, b]: Rgb): { x: number; y: number } {
	const [rr, gg, bb] = [r, g, b].map((c) => linear(c / 255));
	const X = rr * 0.664511 + gg * 0.154324 + bb * 0.162028;
	const Y = rr * 0.283881 + gg * 0.668433 + bb * 0.047685;
	const Z = rr * 0.000088 + gg * 0.07231 + bb * 0.986039;
	const sum = X + Y + Z;
	if (sum === 0) return { x: 0.3127, y: 0.329 }; // D65 white
	return { x: X / sum, y: Y / sum };
}

export function hsvToRgb(h: number, s: number, v: number): Rgb {
	const c = v * s;
	const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
	const k = v - c;
	const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
	return [r, g, b].map((n) => Math.round((n + k) * 255)) as Rgb;
}

function rgbToHs([r, g, b]: Rgb): { hue: number; sat: number } {
	const [rn, gn, bn] = [r / 255, g / 255, b / 255];
	const max = Math.max(rn, gn, bn);
	const delta = max - Math.min(rn, gn, bn);
	let hue = 0;
	if (delta > 0) {
		if (max === rn) hue = 60 * (((gn - bn) / delta) % 6);
		else if (max === gn) hue = 60 * ((bn - rn) / delta + 2);
		else hue = 60 * ((rn - gn) / delta + 4);
		if (hue < 0) hue += 360;
	}
	return { hue, sat: max === 0 ? 0 : delta / max };
}

export const css = ([r, g, b]: Rgb) => `rgb(${r} ${g} ${b})`;

/** Paint the hue/saturation wheel (hue by angle, saturation by distance from the centre). */
export function paintWheel(canvas: HTMLCanvasElement) {
	const ctx = canvas.getContext('2d');
	if (!ctx) return;
	const { width: w, height: h } = canvas;
	const radius = Math.min(w, h) / 2;
	const img = ctx.createImageData(w, h);
	for (let py = 0; py < h; py++) {
		for (let px = 0; px < w; px++) {
			const dx = px - w / 2;
			const dy = py - h / 2;
			const dist = Math.hypot(dx, dy);
			const i = (py * w + px) * 4;
			if (dist > radius) continue;
			const [r, g, b] = hsvToRgb(((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360, dist / radius, 1);
			img.data.set([r, g, b, 255], i);
		}
	}
	ctx.putImageData(img, 0, 0);
}

/** Where a color sits on the wheel, as fractions (0–1) of its box. */
export function wheelPosition(x: number, y: number): { left: number; top: number } {
	const { hue, sat } = rgbToHs(xyToRgb(x, y));
	const rad = (hue * Math.PI) / 180;
	return { left: 0.5 + (sat / 2) * Math.cos(rad), top: 0.5 + (sat / 2) * Math.sin(rad) };
}

/** The color under a point of the wheel (fractions of its box), or null outside it. */
export function wheelColor(left: number, top: number): { x: number; y: number } | null {
	const dx = left - 0.5;
	const dy = top - 0.5;
	const dist = Math.hypot(dx, dy) * 2;
	if (dist > 1) return null;
	return rgbToXy(hsvToRgb(((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360, dist, 1));
}

/** Named colors: the keyboard and screen-reader way to pick one (the wheel is pointer-only). */
export const PRESETS: { name: () => string; rgb: Rgb }[] = [
	{ name: m.color_red, rgb: [239, 68, 68] },
	{ name: m.zigbee_lamps_color_orange, rgb: [249, 115, 22] },
	{ name: m.zigbee_lamps_color_yellow, rgb: [234, 179, 8] },
	{ name: m.zigbee_lamps_color_green, rgb: [34, 197, 94] },
	{ name: m.color_blue, rgb: [59, 130, 246] },
	{ name: m.zigbee_lamps_color_purple, rgb: [168, 85, 247] },
	{ name: m.zigbee_lamps_color_pink, rgb: [236, 72, 153] },
	{ name: m.color_white, rgb: [245, 245, 244] }
];

/** Effects the Hue Zigbee lamps know, by the id the server expects. */
export const EFFECTS: { id: string; name: () => string }[] = [
	{ id: 'candle', name: m.zigbee_lamps_effect_candle },
	{ id: 'fireplace', name: m.zigbee_lamps_effect_fireplace },
	{ id: 'colorloop', name: m.zigbee_lamps_effect_colorloop },
	{ id: 'sunrise', name: m.zigbee_lamps_effect_sunrise },
	{ id: 'sparkle', name: m.zigbee_lamps_effect_sparkle },
	{ id: 'opal', name: m.zigbee_lamps_effect_opal },
	{ id: 'glisten', name: m.zigbee_lamps_effect_glisten },
	{ id: 'blink', name: m.zigbee_lamps_effect_blink },
	{ id: 'breathe', name: m.zigbee_lamps_effect_breathe },
	{ id: 'okay', name: m.zigbee_lamps_effect_okay }
];
export const STOP_EFFECT = 'stop_hue_effect';
