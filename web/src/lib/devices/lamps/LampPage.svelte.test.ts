import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { when } from '#lib/i18n.svelte.ts';
import { htmlOf } from '#lib/test/snippet.ts';
import { hueLamp } from '#lib/test/lamps.ts';
import LampPage from './LampPage.svelte';
import { fromHue, hue, type Lamp } from './lamp.ts';

afterEach(() => forgetAll());

/** The details list as [term, value] pairs. */
const details = () =>
	[...document.querySelectorAll('dl div')].map((d) => [d.querySelector('dt')?.textContent, d.querySelector('dd')?.textContent]);

describe('LampPage', () => {
	it('while loading the title says so, and nothing else shows', async () => {
		await render(LampPage, { lamp: undefined, loading: true, driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent(m.common_loading());
		await expect.element(page.getByRole('link', { name: m.back_home() })).toHaveAttribute('href', '/');
		expect(document.querySelector('dl')).toBeNull();
	});

	it('an unknown lamp: « Lampe introuvable »', async () => {
		await render(LampPage, { lamp: undefined, loading: false, driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent(m.lamps_not_found());
	});

	it('its name as the title, the tile without a link, the white slider and the details', async () => {
		const lamp = fromHue(hueLamp({ lastSeen: '2026-10-02T10:00:00Z' }));
		await render(LampPage, { lamp, loading: false, driver: hue, rows: [['Adresse', 'AA:BB']] });
		await expect.element(page.getByRole('heading', { level: 1 })).toHaveTextContent('Lampe du salon');
		await expect.element(page.getByRole('button', { name: 'Lampe du salon' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'Lampe du salon' })).not.toBeInTheDocument();
		await expect.element(page.getByRole('region', { name: m.lamps_temperature() })).toBeVisible();
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).toBeVisible();
		expect(details()).toEqual([
			[m.device_model(), 'LCA001'],
			[m.lamps_manufacturer(), 'Signify'],
			[m.device_firmware(), '1.104.2'],
			['Adresse', 'AA:BB'],
			[m.lamps_last_seen(), when('2026-10-02T10:00:00Z')]
		]);
	});

	it('says what it does not know', async () => {
		const lamp = fromHue(hueLamp({ model: null, firmware: null, lastSeen: null, state: { temperature: null } }));
		await render(LampPage, { lamp, loading: false, driver: hue });
		await expect.element(page.getByRole('heading', { level: 1 })).toBeVisible();
		expect(details()).toEqual([
			[m.device_model(), m.lamps_unknown_model()],
			[m.lamps_manufacturer(), 'Signify'],
			[m.lamps_last_seen(), m.common_unknown()]
		]);
		// no white tuning: no temperature slider
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).not.toBeInTheDocument();
	});

	it('a family’s colour section replaces the plain slider; notes and extra sections get the lamp', async () => {
		const lamp = fromHue(hueLamp());
		await render(LampPage, {
			lamp,
			loading: false,
			driver: hue,
			colour: htmlOf((l: Lamp) => `<p>Couleur de ${l.name}</p>`),
			notes: htmlOf((l: Lamp) => `<p>Note ${l.id}</p>`),
			children: htmlOf(() => '<p>Masquer</p>')
		});
		await expect.element(page.getByText('Couleur de Lampe du salon')).toBeVisible();
		await expect.element(page.getByText('Note hue-1')).toBeVisible();
		await expect.element(page.getByText('Masquer')).toBeVisible();
		await expect.element(page.getByRole('slider', { name: m.lamps_temperature() })).not.toBeInTheDocument();
	});
});
