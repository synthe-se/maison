import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { tempoForecast, tempoToday } from '#lib/test/tempo.ts';
import TempoTile from './TempoTile.svelte';

const tempo = tempoToday({ today: { date: '2026-12-10', color: 'BLUE' }, tomorrow: { date: '2026-12-11', color: 'RED' } });

describe('TempoTile', () => {
	afterEach(() => forgetAll());

	it('says it is loading under its title', async () => {
		stubApi({ '/tempo': () => new Promise(() => {}) });
		await render(TempoTile);
		await expect.element(page.getByRole('heading', { name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByText(m.common_loading())).toBeInTheDocument();
	});

	it('keeps its place when the server does not answer, and says so', async () => {
		stubApi({ '/tempo': new Response('{"error":"down"}', { status: 502 }) });
		await render(TempoTile);
		await expect.element(page.getByRole('heading', { name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
	});

	it('shows today and tomorrow, and leads to the calendar', async () => {
		stubApi({ '/tempo': tempo, '/tempo/forecast': tempoForecast() });
		await render(TempoTile);
		const group = page.getByRole('region', { name: m.nav_tempo() });
		await expect.element(group.getByRole('article', { name: m.day_today() })).toMatchTextContent(m.color_blue());
		await expect.element(group.getByRole('link', { name: m.tempo_open_page() })).toHaveAttribute('href', '/tempo-predictions');
	});

	it('keeps its place and offers a retry when the server fails', async () => {
		stubApi({ '/tempo': new Response('{"error":"RTE down"}', { status: 503 }) });
		await render(TempoTile);
		await expect.element(page.getByRole('heading', { name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
	});
});
