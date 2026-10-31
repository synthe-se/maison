import { describe, expect, it } from 'vitest';
import { PHOTO_SIZE } from '#lib/limits.ts';
import { cover } from './resize.ts';

describe('cover', () => {
	it('a portrait of the panel’s shape is scaled down whole', () => {
		expect(cover(1600, 2560)).toEqual({ sx: 0, sy: 0, sw: 1600, sh: 2560, ...PHOTO_SIZE });
	});

	it('a landscape keeps its height, its sides cut off evenly', () => {
		const c = cover(4000, 3000);
		expect([c.width, c.height]).toEqual([800, 1280]);
		expect(c.sh).toBe(3000);
		expect(c.sw).toBeCloseTo(1875);
		expect(c.sx).toBeCloseTo((4000 - 1875) / 2);
		expect(c.sy).toBe(0);
	});

	it('a tall panorama keeps its width, its top and bottom cut off evenly', () => {
		const c = cover(1000, 4000);
		expect([c.width, c.height]).toEqual([800, 1280]);
		expect(c.sw).toBe(1000);
		expect(c.sh).toBeCloseTo(1600);
		expect(c.sy).toBeCloseTo(1200);
	});

	it('a picture smaller than the panel is never enlarged', () => {
		expect(cover(400, 300)).toEqual({ sx: 0, sy: 0, sw: 400, sh: 300, width: 400, height: 300 });
	});

	it('one small on one side only stays at its scale, and at most the panel', () => {
		const c = cover(600, 2000);
		expect([c.width, c.height]).toEqual([600, 1280]);
		expect(c.sy).toBe(360);
	});
});
