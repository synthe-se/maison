import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { shutter } from '#lib/test/shutters.ts';
import SunSettings from './SunSettings.svelte';

const lyon = { name: 'Lyon, Auvergne-Rhône-Alpes, France', latitude: 45.75, longitude: 4.85 };

describe('SunSettings', () => {
	afterEach(() => {
		forgetAll();
		vi.unstubAllGlobals();
	});

	it('without a place, asks for the town first, then saves the one picked', async () => {
		const api = stubApi({
			'GET /matter/place': { success: true, place: null },
			'PUT /matter/place': ({ body }: { body: unknown }) => ({ success: true, place: body })
		});
		api.routes[`GET /matter/place/search?q=Lyon&lang=${document.documentElement.lang || 'en'}`] = { success: true, places: [lyon] };
		const searched = () => api.calls.filter((c) => c.path.startsWith('/matter/place/search'));
		render(SunSettings, { cover: shutter(), onchange: () => {} });

		await expect.element(page.getByText(m.place_hint())).toBeVisible();
		await expect.element(page.getByRole('switch', { name: m.shutters_close_at_sunset() })).not.toBeInTheDocument();
		await userEvent.fill(page.getByLabelText(m.place_search()), 'Lyon');
		await expect.poll(() => searched().length).toBe(1); // asked once, after the typing pause
		expect(searched()[0].path).toContain('q=Lyon');
		await page.getByRole('button', { name: lyon.name }).click();
		await expect.poll(() => api.sent('PUT', '/matter/place').length).toBe(1);
		expect(api.sent('PUT', '/matter/place')[0].body).toEqual(lyon);
	});

	it('says when no town matches', async () => {
		const api = stubApi({ 'GET /matter/place': { success: true, place: null } });
		render(SunSettings, { cover: shutter(), onchange: () => {} });
		await expect.element(page.getByText(m.place_hint())).toBeVisible();
		api.routes[`GET /matter/place/search?q=Zzz&lang=${document.documentElement.lang || 'en'}`] = { success: true, places: [] };
		await userEvent.fill(page.getByLabelText(m.place_search()), 'Zzz');
		await expect.element(page.getByText(m.place_none())).toBeVisible();
		expect(api.sent('PUT', '/matter/place')).toHaveLength(0);
	});

	it('with a place: closing at sunset starts an hour after it, and the offset can change', async () => {
		const changed = vi.fn();
		const api = stubApi({
			'GET /matter/place': { success: true, place: lyon },
			'PUT /matter/covers/s1/schedule': ({ body }: { body: unknown }) => ({ success: true, cover: shutter({ schedule: body as never }) })
		});
		const { rerender } = await render(SunSettings, { cover: shutter(), onchange: changed });

		await expect.element(page.getByText(lyon.name)).toBeVisible();
		await page.getByRole('switch', { name: m.shutters_close_at_sunset() }).click();
		await expect.poll(() => api.sent('PUT', '/matter/covers/s1/schedule').length).toBe(1);
		const sent = api.sent('PUT', '/matter/covers/s1/schedule')[0].body as Record<string, unknown>;
		expect(sent).toMatchObject({ closeAtSunset: true, sunsetOffsetMin: 60, openAtSunrise: false });
		expect(changed).toHaveBeenCalledOnce();

		const next = '2026-10-02T18:28:00Z';
		await rerender({ cover: shutter({ schedule: sent as never, nextClose: next }) });
		await expect.element(page.getByText(new RegExp(m.shutters_next_close({ when: '' }).trim()))).toBeVisible();
		await expect.element(page.getByRole('button', { name: new RegExp(m.shutters_offset()) })).toBeVisible();
	});
});
