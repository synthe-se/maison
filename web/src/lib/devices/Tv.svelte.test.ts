import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { clock } from '#lib/i18n.svelte.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import type { TvStatus, TvStatusResponse } from '#lib/api.ts';
import { json, sentBody, stubFetch, type FetchCall } from '#lib/test/fetch.ts';
import Tv from './Tv.svelte';

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
function serve(status: TvStatusResponse | (() => Response), answers: Record<string, () => Response | Promise<Response>> = {}) {
	return stubFetch((url, init) => {
		const key = `${init?.method ?? 'GET'} ${url}`;
		if (answers[key]) return answers[key]();
		if (key === 'GET /api/tv') return typeof status === 'function' ? status() : json(status);
		if (key === 'POST /api/tv/power') return json({ success: true, power: sentBody({ url, init }).state === 'on' ? 'on' : 'standby' });
		if (key === 'PUT /api/tv/volume') {
			const b = sentBody({ url, init });
			return json({ success: true, volume: { current: b.level ?? 12, min: 0, max: 60, muted: b.muted ?? false } });
		}
		if (key === 'POST /api/tv/ambilight') return json({ success: true, ambilight: { power: true } });
		return json({ success: true, message: 'ok' });
	});
}

const reads = (calls: FetchCall[]) => calls.filter((c) => c.url === '/api/tv' && (c.init?.method ?? 'GET') === 'GET').length;
const sent = (calls: FetchCall[]) => calls.filter((c) => c.init?.method && c.init.method !== 'GET').map((c) => [c.init!.method, c.url, sentBody(c)]);
/** A keyboard activation of a key (a click with no pointer: sends once). */
async function activate(name: string) {
	const key = page.getByRole('button', { name, exact: true });
	await expect.element(key).toBeInTheDocument();
	key.element().dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }));
}

beforeEach(() => {
	// wide screen: the pad and the rarer keys are unfolded
	vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList);
	localStorage.removeItem(ORDER_KEY);
});
afterEach(() => {
	forgetAll();
	localStorage.removeItem(ORDER_KEY);
});

describe('Tv', () => {
	it('says it is loading, then reads the TV exactly once', async () => {
		let answer!: (r: Response) => void;
		const calls = serve(() => new Promise<Response>((r) => (answer = r)) as unknown as Response);
		await render(Tv);
		await expect.element(page.getByRole('status')).toHaveTextContent(m.common_loading());
		answer(json(tvStatus()));
		await expect.element(page.getByRole('button', { name: NAME })).toBeVisible();
		await new Promise((r) => setTimeout(r, 200));
		expect(calls.map((c) => c.url)).toEqual(['/api/tv']);
	});

	it('on: a pressed toggle named after the set, the state and the volume in words', async () => {
		serve(tvStatus());
		await render(Tv);
		await expect.element(page.getByRole('button', { name: NAME })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(m.meross_state_on(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tv_volume_fact({ level: 12 }))).toBeVisible();
		await expect.element(page.getByRole('group', { name: m.tv_pad({ name: NAME }) })).toBeVisible();
		await expect.element(page.getByRole('slider', { name: m.tv_volume() })).toHaveAttribute('aria-valuetext', m.tv_volume_value({ level: 12, max: 60 }));
	});

	it('falls back to « Télévision » when the set has no name; muted says so', async () => {
		serve(tvStatus({ name: undefined, volume: { current: 3, min: 0, max: 60, muted: true } }));
		await render(Tv);
		await expect.element(page.getByRole('button', { name: m.tv_title(), exact: true })).toBeVisible();
		await expect.element(page.getByText(m.tv_muted())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.tv_mute() })).toHaveAttribute('aria-pressed', 'true');
	});

	it('turning it off: one POST over infrared, then a single re-read', async () => {
		const calls = serve(tvStatus());
		await render(Tv);
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
			'PUT /api/tv/volume': async () => {
				await gate;
				return json({ success: true, volume: { current: 14, min: 0, max: 60, muted: false } });
			}
		});
		await render(Tv);
		await activate(m.tv_volume_up());
		await activate(m.tv_volume_up());
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
		await render(Tv);
		await activate(m.tv_volume_down());
		await activate(m.tv_mute());
		await expect.poll(() => sent(calls).length).toBe(2);
		expect(sent(calls)).toEqual([
			['PUT', '/api/tv/volume', { level: 0 }],
			['PUT', '/api/tv/volume', { muted: true }]
		]);
		expect(reads(calls)).toBe(1);
	});

	it('pad keys go out in JointSPACE’s names, with no status read', async () => {
		const calls = serve(tvStatus());
		await render(Tv);
		for (const name of [m.remote_keys_up(), m.tv_key_ok(), m.tv_key_back(), m.remote_keys_home(), m.tv_key_source(), m.tv_key_play_pause()]) await activate(name);
		const pad = page.getByRole('group', { name: m.tv_pad({ name: NAME }) }).element() as HTMLElement;
		pad.focus();
		await userEvent.keyboard('{ArrowLeft}');
		await expect.poll(() => sent(calls).length).toBe(7);
		expect(sent(calls).map(([, , b]) => b.key)).toEqual(['cursor_up', 'confirm', 'back', 'home', 'source', 'play_pause', 'cursor_left']);
		expect(sent(calls).every(([method, url]) => method === 'POST' && url === '/api/tv/key')).toBe(true);
		expect(reads(calls)).toBe(1);
	});

	it('the volume slider sends a level', async () => {
		const calls = serve(tvStatus());
		await render(Tv);
		const slider = page.getByRole('slider', { name: m.tv_volume() });
		await expect.element(slider).toBeInTheDocument();
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{PageUp}');
		await expect.poll(() => sent(calls)).toEqual([['PUT', '/api/tv/volume', { level: 22 }]]);
		expect(reads(calls)).toBe(1);
	});

	it('Ambilight is a pressed toggle that follows the answer', async () => {
		const calls = serve(tvStatus());
		await render(Tv);
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
		await render(Tv);
		await page.getByRole('button', { name: m.tv_switch_to_box() }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/tv/source/box', null]]);
		expect(toast).toHaveBeenCalledWith(m.tv_switched_to_box());
	});

	it('JointSPACE silent: the state is assumed, two buttons and the last order instead of a toggle', async () => {
		const at = new Date(2026, 9, 2, 18, 2).getTime();
		localStorage.setItem(ORDER_KEY, JSON.stringify({ on: true, at }));
		const calls = serve(tvStatus({ volume: undefined, ambilight: undefined }));
		await render(Tv);
		await expect.element(page.getByText(m.tv_assumed())).toBeVisible();
		await expect.element(page.getByText(m.tv_last_order_on({ time: clock(at) }))).toBeVisible();
		await expect.element(page.getByRole('button', { name: NAME })).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.tv_more_controls() })).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.action_turn_off() }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/tv/power', { state: 'off', switchToBox: true }]]);
		await expect.element(page.getByText(m.tv_last_order_off({ time: clock(JSON.parse(localStorage.getItem(ORDER_KEY)!).at) }))).toBeVisible();
	});

	it('assumed, turning on goes over infrared too', async () => {
		const calls = serve(tvStatus({ volume: undefined }));
		await render(Tv);
		await page.getByRole('button', { name: m.action_turn_on() }).click();
		await expect.poll(() => sent(calls)).toEqual([['POST', '/api/tv/power', { state: 'on', switchToBox: true }]]);
	});

	it('deep standby: says why waking takes time, then shows the wake-up progress', async () => {
		const calls = serve(tvStatus({ power: 'deep_standby', volume: undefined }), { 'POST /api/tv/power': () => new Promise<Response>(() => {}) });
		await render(Tv);
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
		serve(tvStatus({ power: 'standby', volume: undefined }), { 'POST /api/tv/power': () => new Promise<Response>(() => {}) });
		await render(Tv);
		await expect.element(page.getByText(m.litter_box_status_standby(), { exact: true })).toBeVisible();
		await page.getByRole('button', { name: NAME }).click();
		await expect.element(page.getByRole('button', { name: NAME })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('progressbar')).not.toBeInTheDocument();
	});

	it('not configured: says so and opens its settings; saving sends the config and re-reads once', async () => {
		const calls = serve(tvStatus({ configured: false, power: 'standby', volume: undefined }));
		await render(Tv);
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
		await render(Tv);
		const settings = page.getByRole('button', { name: m.common_settings() });
		await expect.element(settings).toHaveAttribute('aria-expanded', 'false');
		await settings.click();
		await expect.element(page.getByLabelText(m.tv_host())).toHaveValue('192.168.1.52');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.element(settings).toHaveAttribute('aria-expanded', 'false');
		expect(sent(calls)[0][1]).toBe('/api/tv/config');
	});

	it('no answer at all: the tile says so', async () => {
		serve(() => json({ success: false, error: 'TV unreachable' }, 503));
		await render(Tv);
		await expect.element(page.getByText(m.command_no_answer_short())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.tv_title() })).not.toBeInTheDocument();
	});
});
