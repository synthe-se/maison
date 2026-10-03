import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { AndroidTvStatus, AndroidTvStatusResponse, TvStatus, TvStatusResponse } from '#lib/devices/tv/api.ts';
import { stubApi } from '#lib/test/api.ts';
import TvGroup from './TvGroup.svelte';
import TvTile from './tv/TvTile.svelte';
import BoxTile from './tv/BoxTile.svelte';

const NAME = 'Philips 55PUS';
const tvStatus = (over: Partial<TvStatus> = {}): TvStatusResponse => ({
	success: true,
	config: { host: '192.168.1.52' },
	status: {
		configured: true,
		power: 'on',
		name: NAME,
		volume: { current: 12, min: 0, max: 60, muted: false },
		ambilight: { power: false },
		...over
	}
});
const boxStatus = (over: Partial<AndroidTvStatus> = {}): AndroidTvStatusResponse => ({
	success: true,
	config: { host: '192.168.1.153' },
	status: { configured: true, reachable: true, awake: true, currentApp: 'org.smarttube.beta', model: 'LEAP-S1', paired: true, ...over }
});
/** A keyboard activation of a key (a click with no pointer: sends once). */
const activate = (name: string) =>
	page
		.getByRole('button', { name, exact: true })
		.element()
		.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }));

describe('TvGroup (the dashboard group « Télé »)', () => {
	it('the TV and the box under one title, each leading to its page; no remote keymap', async () => {
		stubApi({ '/tv': tvStatus(), '/androidtv': boxStatus() });
		const { container } = await render(TvGroup);
		const group = page.getByRole('region', { name: m.tv_group_title() });
		await expect.element(group.getByRole('link', { name: NAME })).toHaveAttribute('href', '/tv');
		await expect.element(group.getByRole('link', { name: 'LEAP-S1' })).toHaveAttribute('href', '/androidtv');
		expect(container.querySelectorAll('h2')).toHaveLength(1);
		await expect.element(page.getByRole('link', { name: m.remote_title() })).not.toBeInTheDocument();
		expect(container.querySelector('a[href="/remote"]')).toBeNull();
	});

	it('one device that cannot be read leaves the other one', async () => {
		stubApi({ '/tv': new Error('down'), '/androidtv': boxStatus() });
		await render(TvGroup);
		await expect.element(page.getByRole('link', { name: 'LEAP-S1' })).toBeVisible();
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
	});
});

describe('TvTile', () => {
	it('power and volume − / + only; the name leads to the TV’s page (the pad lives there)', async () => {
		stubApi({ '/tv': tvStatus() });
		await render(TvTile);
		await expect.element(page.getByRole('button', { name: NAME })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: NAME })).toHaveAttribute('href', '/tv');
		await expect.element(page.getByRole('button', { name: m.key_volume_up() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.key_volume_down() })).toBeVisible();
		await expect.element(page.getByRole('group', { name: m.tv_pad({ name: NAME }) })).not.toBeInTheDocument();
		await expect.element(page.getByRole('slider')).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.tv_ambilight() })).not.toBeInTheDocument();
	});

	it('volume + sends the next level', async () => {
		const api = stubApi({ '/tv': tvStatus(), 'PUT /tv/volume': { success: true, volume: { current: 13, min: 0, max: 60, muted: false } } });
		await render(TvTile);
		await expect.element(page.getByRole('button', { name: m.key_volume_up() })).toBeVisible();
		activate(m.key_volume_up());
		await expect.poll(() => api.sent('PUT', '/tv/volume').map((c) => c.body)).toEqual([{ level: 13 }]);
	});

	it('its state assumed: « Allumer » and « Éteindre », no pressed icon', async () => {
		stubApi({ '/tv': tvStatus({ volume: undefined }) });
		await render(TvTile);
		await expect.element(page.getByText(m.tv_assumed())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.action_turn_on() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: NAME })).not.toBeInTheDocument();
	});
});

describe('BoxTile', () => {
	it('wake/sleep and the app chips only; the name leads to the box’s page (the pad lives there)', async () => {
		stubApi({ '/androidtv': boxStatus() });
		await render(BoxTile);
		await expect.element(page.getByRole('button', { name: 'LEAP-S1', exact: true })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'LEAP-S1' })).toHaveAttribute('href', '/androidtv');
		await expect
			.element(page.getByRole('group', { name: m.android_tv_apps() }).getByRole('button', { name: 'SmartTube' }))
			.toHaveAttribute('aria-current', 'true');
		await expect.element(page.getByRole('group', { name: m.tv_pad({ name: 'LEAP-S1' }) })).not.toBeInTheDocument();
		await expect.element(page.getByText(m.android_tv_paired())).toBeVisible();
	});

	it('an app chip launches the app', async () => {
		const api = stubApi({ '/androidtv': boxStatus(), 'POST /androidtv/launch': { success: true } });
		await render(BoxTile);
		await page.getByRole('button', { name: 'Iris' }).click();
		await expect
			.poll(() => api.sent('POST', '/androidtv/launch').map((c) => c.body))
			.toEqual([{ package: 'studio.kahn.iris.tv', ensureTvOn: true }]);
	});

	it('unreachable: the chips stay, unavailable, saying why', async () => {
		const api = stubApi({ '/androidtv': boxStatus({ reachable: false }) });
		await render(BoxTile);
		const chip = page.getByRole('button', { name: 'Iris' });
		await expect.element(chip).toHaveAttribute('aria-disabled', 'true');
		await expect.element(chip).toHaveAccessibleDescription(m.android_tv_apps_unreachable());
		(chip.element() as HTMLElement).click();
		expect(api.sent('POST', '/androidtv/launch')).toHaveLength(0);
	});
});
