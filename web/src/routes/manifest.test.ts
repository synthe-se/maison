import { describe, expect, it } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { TEMPLATE_IDS } from '#lib/devices/scenes/scenes.ts';

const root = new URL('../../', import.meta.url).pathname;
const manifest = JSON.parse(readFileSync(`${root}static/manifest.webmanifest`, 'utf8'));

describe('the app’s shortcuts', () => {
	it('« Je pars » and « Nuit » ask for the templates’ scenes, Tempo and the feeder open their pages', () => {
		const urls = manifest.shortcuts.map((s: { url: string }) => s.url);
		expect(urls).toEqual([`/?scene=${TEMPLATE_IDS.leave}`, `/?scene=${TEMPLATE_IDS.night}`, '/tempo-predictions', '/device/feeder']);
	});

	it('each has an icon that exists', () => {
		for (const s of manifest.shortcuts) for (const icon of s.icons) expect(existsSync(`${root}static${icon.src}`), icon.src).toBe(true);
	});
});
