import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { time } from '#lib/clock.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { tempoForecast, tempoToday } from '#lib/test/tempo.ts';
import TempoGroup from './TempoGroup.svelte';

const tempo = tempoToday({ today: { date: '2026-12-10', color: 'BLUE' }, tomorrow: { date: '2026-12-11', color: 'RED' } });

describe('TempoGroup', () => {
	it('says it is loading under its title', async () => {
		stubApi({ '/tempo': () => new Promise(() => {}) });
		await render(TempoGroup);
		await expect.element(page.getByRole('heading', { name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByText(m.common_loading())).toBeInTheDocument();
	});

	it('keeps its place when the server does not answer, and says so', async () => {
		stubApi({ '/tempo': new Response('{"error":"down"}', { status: 502 }) });
		await render(TempoGroup);
		await expect.element(page.getByRole('heading', { name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
	});

	it('shows today and tomorrow, and leads to the Tempo page in one tap (no second button)', async () => {
		time.now = new Date('2026-12-10T12:00:00');
		stubApi({ '/tempo': tempo, '/tempo/forecast': tempoForecast() });
		await render(TempoGroup);
		const group = page.getByRole('region', { name: m.nav_tempo() });
		await expect.element(group.getByRole('article')).toMatchTextContent(m.color_blue());
		await expect.element(group.getByRole('article')).toMatchTextContent(m.tempo_tomorrow_is({ words: m.color_red() }));
		await expect.element(group.getByRole('link')).toHaveAttribute('href', '/tempo-predictions');
		await expect.element(group.getByRole('link')).toHaveLength(1);
		time.now = new Date();
	});

	it('keeps its place and offers a retry when the server fails', async () => {
		stubApi({ '/tempo': new Response('{"error":"RTE down"}', { status: 503 }) });
		await render(TempoGroup);
		await expect.element(page.getByRole('heading', { name: m.nav_tempo() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
	});
});
