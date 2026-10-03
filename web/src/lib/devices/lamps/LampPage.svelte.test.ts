import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { when } from '#lib/i18n.svelte.ts';
import { htmlOf } from '#lib/test/snippet.ts';
import { hueLamp } from '#lib/test/lamps.ts';
import { ApiError } from '#lib/api.ts';
import type { Loadable } from '#lib/live.svelte.ts';
import LampPage from './LampPage.svelte';
import { fromHue, hue, type Lamp } from './lamp.ts';

/** The lamp's live value as the page reads it. */
const status = (over: Partial<Loadable> = {}): Loadable => ({
	loading: false,
	failed: false,
	stale: false,
	at: 0,
	error: undefined,
	refresh: async () => {},
	...over
});

/** The details list as [term, value] pairs. */
const details = () =>
	[...document.querySelectorAll('dl div')].map((d) => [d.querySelector('dt')?.textContent, d.querySelector('dd')?.textContent]);

describe('LampPage', () => {
	it('while loading the title says so, and nothing else shows', async () => {
		await render(LampPage, { lamp: undefined, status: status({ loading: true }), driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent(m.common_loading());
		await expect.element(page.getByRole('link', { name: m.back_home() })).toHaveAttribute('href', '/');
		expect(document.querySelector('dl')).toBeNull();
	});

	it('an unknown lamp (the server answers 404): « Lampe introuvable », nothing to retry', async () => {
		await render(LampPage, { lamp: undefined, status: status({ failed: true, error: new ApiError('', 404, 'not_found') }), driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent(m.lamps_not_found());
		await expect.element(page.getByText(m.lamps_not_found()).nth(1)).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).not.toBeInTheDocument();
	});

	it('a lamp that cannot be read (no answer, a 5xx) says so, with a retry, never « not found »', async () => {
		await render(LampPage, { lamp: undefined, status: status({ failed: true, error: new ApiError('', 502) }), driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent(m.lamps_lamp());
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
		await expect.element(page.getByText(m.lamps_not_found())).not.toBeInTheDocument();
	});

	it('its name as the title, the tile without a link, the white slider and the details', async () => {
		const lamp = fromHue(hueLamp({ lastSeen: '2026-10-02T10:00:00Z' }));
		await render(LampPage, { lamp, status: status(), driver: hue, rows: [['Adresse', 'AA:BB']] });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Lampe du salon');
		await expect.element(page.getByRole('button', { name: 'Lampe du salon' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'Lampe du salon' })).not.toBeInTheDocument();
		await expect.element(page.getByRole('region', { name: m.lamps_temperature() })).toBeVisible();
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeVisible();
		expect(details()).toEqual([
			[m.device_model(), 'LCA001'],
			[m.lamps_manufacturer(), 'Signify'],
			[m.lamps_radio(), m.lamps_radio_bluetooth()],
			[m.device_firmware(), '1.104.2'],
			['Adresse', 'AA:BB'],
			[m.lamps_last_seen(), when('2026-10-02T10:00:00Z')]
		]);
	});

	it('says what it does not know', async () => {
		const lamp = fromHue(hueLamp({ model: null, firmware: null, lastSeen: null, state: { temperature: null } }));
		await render(LampPage, { lamp, status: status(), driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toBeVisible();
		expect(details()).toEqual([
			[m.device_model(), m.lamps_unknown_model()],
			[m.lamps_manufacturer(), 'Signify'],
			[m.lamps_radio(), m.lamps_radio_bluetooth()],
			[m.lamps_last_seen(), m.common_unknown()]
		]);
		// no white tuning: no temperature slider
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).not.toBeInTheDocument();
	});

	it('a family’s color section replaces the plain slider; notes and extra sections get the lamp', async () => {
		const lamp = fromHue(hueLamp());
		await render(LampPage, {
			lamp,
			status: status(),
			driver: hue,
			color: htmlOf((l: Lamp) => `<p>Couleur de ${l.name}</p>`),
			notes: htmlOf((l: Lamp) => `<p>Note ${l.id}</p>`),
			children: htmlOf(() => '<p>Masquer</p>')
		});
		await expect.element(page.getByText('Couleur de Lampe du salon')).toBeVisible();
		await expect.element(page.getByText('Note hue-1')).toBeVisible();
		await expect.element(page.getByText('Masquer')).toBeVisible();
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).not.toBeInTheDocument();
	});
});
