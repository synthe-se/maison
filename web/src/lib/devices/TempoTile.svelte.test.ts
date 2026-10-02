import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import TempoTile from './TempoTile.svelte';

const tempo = { success: true, today: { date: '2026-12-10', color: 'BLUE' }, tomorrow: { date: '2026-12-11', color: 'RED' }, tarifs: null, message: '' };

describe('TempoTile', () => {
	afterEach(() => forgetAll());

	it('says it is loading under its title', async () => {
		stubApi({ '/tempo': () => new Promise(() => {}) });
		await render(TempoTile);
		await expect.element(page.getByRole('heading', { name: m.tempo_title() })).toBeVisible();
		await expect.element(page.getByRole('status')).toHaveTextContent(m.common_loading());
	});

	it('shows today and tomorrow, and leads to the calendar', async () => {
		stubApi({ '/tempo': tempo, '/tempo/predictions': { success: true, predictions: [] } });
		await render(TempoTile);
		const group = page.getByRole('region', { name: m.tempo_title() });
		await expect.element(group.getByRole('article', { name: m.day_today() })).toMatchTextContent(m.color_blue());
		await expect.element(group.getByRole('link', { name: m.tempo_open_calendar() })).toHaveAttribute('href', '/tempo-predictions');
	});

	it('is not shown at all when the server has no Tempo', async () => {
		const api = stubApi({ '/tempo': { success: false, error: 'RTE credentials missing', message: '' } });
		const { container } = await render(TempoTile);
		await expect.poll(() => api.calls).toHaveLength(1);
		await expect.element(page.getByRole('heading', { name: m.tempo_title() })).not.toBeInTheDocument();
		expect(container.textContent?.trim()).toBe('');
	});
});
