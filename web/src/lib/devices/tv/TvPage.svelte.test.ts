import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { clock } from '#lib/i18n.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import type { TvStatus, TvStatusResponse } from '#lib/devices/tv/api.ts';
import { json } from '#lib/test/fetch.ts';
import { stubApi, type ApiCall, type Reply } from '#lib/test/api.ts';
import TvPage from './TvPage.svelte';

const ORDER_KEY = 'maison-tv-last-order';
const NAME = 'Philips 55PUS';

function tvStatus(over: Partial<TvStatus> = {}): TvStatusResponse {
	return {
		success: true,
		config: { host: '192.168.1.52', irBlasterHost: '192.168.1.73', boxHost: null },
		status: {
			configured: true,
			power: 'on',
			name: NAME,
			volume: { current: 12, min: 0, max: 60, muted: false },
			ambilight: { power: false },
			...over
		}
	};
}

/** The TV backend: answers each endpoint like the server, records every call. */
function serve(status: TvStatusResponse | (() => Response), answers: Record<string, Reply> = {}) {
	return stubApi(
		{
			'GET /tv': status,
			'POST /tv/power': (c: ApiCall) => ({ success: true, power: (c.body as { state: string }).state === 'on' ? 'on' : 'standby' }),
			'PUT /tv/volume': (c: ApiCall) => {
				const b = c.body as { level?: number; muted?: boolean };
				return { success: true, volume: { current: b.level ?? 12, min: 0, max: 60, muted: b.muted ?? false } };
			},
			'POST /tv/ambilight': { success: true, ambilight: { power: true } },
			...answers
		},
		{ success: true, message: 'ok' }
	);
}

type Api = ReturnType<typeof stubApi>;
const reads = (api: Api) => api.sent('GET', '/tv').length;
/** The commands sent, as [method, URL, body]. */
const sent = (api: Api) =>
	api.calls
		.filter((c) => c.method !== 'GET')
		.map((c) => [c.method, `/api${c.path}`, (c.body ?? null) as Record<string, unknown> | null] as const);
/** A keyboard activation of a key (a click with no pointer: sends once). */
async function activate(name: string) {
	const key = page.getByRole('button', { name, exact: true });
	await expect.element(key).toBeInTheDocument();
	key.element().dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }));
}

beforeEach(() => {
	session.adopt(leonard);
	localStorage.removeItem(ORDER_KEY);
});
afterEach(() => {
	localStorage.removeItem(ORDER_KEY);
});

describe('TvPage', () => {
	it('says it is loading, then reads the TV exactly once', async () => {
		let answer!: (r: Response) => void;
		const calls = serve(() => new Promise<Response>((r) => (answer = r)) as unknown as Response);
		await render(TvPage);
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
		answer(json(tvStatus()));
		await expect.element(page.getByRole('button', { name: NAME })).toBeVisible();
		await new Promise((r) => setTimeout(r, 200));
		expect(calls.calls.map((c) => c.path)).toEqual(['/tv']);
	});

	it('on: a pressed toggle named after the set, the state and the volume in words', async () => {
		serve(tvStatus());
		await render(TvPage);
		await expect.element(page.getByRole('heading', { level: 1, name: NAME })).toBeVisible();
		await expect.element(page.getByRole('link', { name: m.back_home() })).toHaveAttribute('href', '/');
		await expect.element(page.getByRole('button', { name: NAME })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(m.state_on(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tv_volume_fact({ level: 12 }))).toBeVisible();
		await expect.element(page.getByRole('group', { name: m.tv_pad({ name: NAME }) })).toBeVisible();
		await expect
			.element(page.getByRole('slider', { name: m.tv_volume() }))
			.toHaveAttribute('aria-valuetext', m.tv_volume_value({ level: 12, max: 60 }));
	});

	it('falls back to « TV du salon » (not the group’s name) when the set has none; muted says so', async () => {
		serve(tvStatus({ name: undefined, volume: { current: 3, min: 0, max: 60, muted: true } }));
		await render(TvPage);
		await expect.element(page.getByRole('button', { name: m.tv_default_name(), exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tv_muted())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.key_mute() })).toHaveAttribute('aria-pressed', 'true');
	});

	it('turning it off: one POST over infrared, then a single re-read', async () => {
		const calls = serve(tvStatus());
		await render(TvPage);
		await page.getByRole('button', { name: NAME }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/tv/power', { state: 'off', switchToBox: true }]]);
		expect(JSON.parse(localStorage.getItem(ORDER_KEY)!)).toMatchObject({ on: false });
		await new Promise((r) => setTimeout(r, 200));
		expect(reads(calls)).toBe(2);
	});

	it('a volume step sends the next level, never re-reads the status; quick taps add up', async () => {
		let release!: () => void;
		const gate = new Promise<void>((r) => (release = r));
		const calls = serve(tvStatus(), {
			'PUT /tv/volume': async () => {
				await gate;
				return json({ success: true, volume: { current: 14, min: 0, max: 60, muted: false } });
			}
		});
		await render(TvPage);
		await activate(m.key_volume_up());
		await activate(m.key_volume_up());
		await expect.element(page.getByText('14', { exact: true })).toBeInTheDocument();
		release();
		await expect.element(page.getByText(m.tv_volume_fact({ level: 14 }))).toBeVisible();
		expect(sent(calls)).toEqual([
			['PUT', '/api/tv/volume', { level: 13 }],
			['PUT', '/api/tv/volume', { level: 14 }]
		]);
		expect(reads(calls)).toBe(1);
	});

	it('volume − stops at the minimum; mute flips the muted state', async () => {
		const calls = serve(tvStatus({ volume: { current: 0, min: 0, max: 60, muted: false } }));
		await render(TvPage);
		await activate(m.key_volume_down());
		await activate(m.key_mute());
		await expect.poll(() => sent(calls).length).toBe(2);
		expect(sent(calls)).toEqual([
			['PUT', '/api/tv/volume', { level: 0 }],
			['PUT', '/api/tv/volume', { muted: true }]
		]);
		expect(reads(calls)).toBe(1);
	});

	it('pad keys go out in JointSPACE’s names, with no status read', async () => {
		const calls = serve(tvStatus());
		await render(TvPage);
		for (const name of [m.key_up(), m.key_ok(), m.key_back(), m.nav_home(), m.key_source(), m.key_play_pause()]) await activate(name);
		const pad = page.getByRole('group', { name: m.tv_pad({ name: NAME }) }).element() as HTMLElement;
		pad.focus();
		await userEvent.keyboard('{ArrowLeft}');
		await expect.poll(() => sent(calls).length).toBe(7);
		expect(sent(calls).map(([, , b]) => b?.key)).toEqual(['cursor_up', 'confirm', 'back', 'home', 'source', 'play_pause', 'cursor_left']);
		expect(sent(calls).every(([method, url]) => method === 'POST' && url === '/api/tv/key')).toBe(true);
		expect(reads(calls)).toBe(1);
	});

	it('the volume slider sends a level', async () => {
		const calls = serve(tvStatus());
		await render(TvPage);
		const slider = page.getByRole('slider', { name: m.tv_volume() });
		await expect.element(slider).toBeInTheDocument();
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{PageUp}');
		await expect.poll(() => sent(calls)).toEqual([['PUT', '/api/tv/volume', { level: 22 }]]);
		expect(reads(calls)).toBe(1);
	});

	it('Ambilight is a pressed toggle that follows the answer', async () => {
		const calls = serve(tvStatus());
		await render(TvPage);
		const button = page.getByRole('button', { name: m.tv_ambilight() });
		await expect.element(button).toHaveAttribute('aria-pressed', 'false');
		await button.click();
		await expect.element(button).toHaveAttribute('aria-pressed', 'true');
		expect(sent(calls)).toEqual([['POST', '/api/tv/ambilight', { state: 'toggle' }]]);
		expect(reads(calls)).toBe(1);
	});

	it('switching to the box: one POST, a toast, a single re-read', async () => {
		const toast = vi.spyOn(ui, 'toast');
		const calls = serve(tvStatus());
		await render(TvPage);
		await page.getByRole('button', { name: m.tv_switch_to_box() }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/tv/source/box', null]]);
		expect(toast).toHaveBeenCalledWith(m.tv_switched_to_box());
	});

	it('JointSPACE silent: the state is assumed, two buttons and the last order instead of a toggle', async () => {
		const at = new Date(2026, 9, 2, 18, 2).getTime();
		localStorage.setItem(ORDER_KEY, JSON.stringify({ on: true, at }));
		const calls = serve(tvStatus({ volume: undefined, ambilight: undefined }));
		await render(TvPage);
		await expect.element(page.getByText(m.tv_assumed())).toBeVisible();
		await expect.element(page.getByText(m.tv_last_order_on({ time: clock(at) }))).toBeVisible();
		await expect.element(page.getByRole('button', { name: NAME })).not.toBeInTheDocument();
		await expect.element(page.getByRole('group', { name: m.tv_pad({ name: NAME }) })).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.action_turn_off() }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/tv/power', { state: 'off', switchToBox: true }]]);
		await expect
			.element(page.getByText(m.tv_last_order_off({ time: clock(JSON.parse(localStorage.getItem(ORDER_KEY)!).at) })))
			.toBeVisible();
	});

	it('assumed, turning on goes over infrared too', async () => {
		const calls = serve(tvStatus({ volume: undefined }));
		await render(TvPage);
		await page.getByRole('button', { name: m.action_turn_on() }).click();
		await expect.poll(() => sent(calls)).toEqual([['POST', '/api/tv/power', { state: 'on', switchToBox: true }]]);
	});

	it('deep standby: says why waking takes time, then shows the wake-up progress', async () => {
		const calls = serve(tvStatus({ power: 'deep_standby', volume: undefined }), {
			'POST /tv/power': () => new Promise<Response>(() => {})
		});
		await render(TvPage);
		await expect.element(page.getByText(m.tv_deep_standby(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tv_deep_standby_hint())).toBeVisible();
		const toggle = page.getByRole('button', { name: NAME });
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
		await toggle.click();
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'true');
		const bar = page.getByRole('progressbar', { name: m.tv_waking() });
		await expect.element(bar).toHaveAttribute('aria-valuemax', '30');
		await expect.element(bar).toHaveAttribute('aria-valuetext', m.tv_waking_elapsed({ seconds: 0 }));
		await expect.poll(() => bar.element().getAttribute('aria-valuetext'), { timeout: 3000 }).toBe(m.tv_waking_elapsed({ seconds: 1 }));
		expect(sent(calls)).toEqual([['POST', '/api/tv/power', { state: 'on', switchToBox: true }]]);
	});

	it('standby: the toggle is not pressed, and waking from standby shows no bar', async () => {
		serve(tvStatus({ power: 'standby', volume: undefined }), { 'POST /tv/power': () => new Promise<Response>(() => {}) });
		await render(TvPage);
		await expect.element(page.getByText(m.state_standby(), { exact: true })).toBeVisible();
		await page.getByRole('button', { name: NAME }).click();
		await expect.element(page.getByRole('button', { name: NAME })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('progressbar')).not.toBeInTheDocument();
	});

	it('not configured: says so and opens its settings; saving sends the config and re-reads once', async () => {
		const calls = serve(tvStatus({ configured: false, power: 'standby', volume: undefined }));
		await render(TvPage);
		await expect.element(page.getByText(m.tv_not_configured())).toBeVisible();
		await expect.element(page.getByText(m.tv_configure_hint())).toBeVisible();
		await expect.element(page.getByRole('button', { name: NAME })).not.toBeInTheDocument();
		await userEvent.fill(page.getByLabelText(m.tv_box_host()).element(), '192.168.1.153');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([
			['PUT', '/api/tv/config', { host: '192.168.1.52', irBlasterHost: '192.168.1.73', boxHost: '192.168.1.153' }]
		]);
	});

	it('the settings button unfolds and folds the settings', async () => {
		const calls = serve(tvStatus());
		await render(TvPage);
		const settings = page.getByRole('button', { name: m.common_settings_of({ name: NAME }) });
		await expect.element(settings).toHaveAttribute('aria-expanded', 'false');
		await settings.click();
		await expect.element(page.getByLabelText(m.tv_host())).toHaveValue('192.168.1.52');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.element(settings).toHaveAttribute('aria-expanded', 'false');
		// the form is gone: the focus is back on the button that opened it
		await expect.element(settings).toHaveFocus();
		expect(sent(calls)[0][1]).toBe('/api/tv/config');
	});

	it('a member: no settings button; unconfigured, says an admin sets it up', async () => {
		session.adopt(alex);
		serve(tvStatus({ configured: false, power: 'standby', volume: undefined }));
		await render(TvPage);
		await expect.element(page.getByText(m.tv_configure_admin())).toBeVisible();
		await expect.element(page.getByLabelText(m.tv_host())).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: /Réglages/ })).not.toBeInTheDocument();
	});

	it('no answer at all: the tile says so', async () => {
		serve(() => json({ success: false, error: 'TV unreachable' }, 503));
		await render(TvPage);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
	});
});
