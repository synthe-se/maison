import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import type { AndroidTvConfig, AndroidTvStatus, AndroidTvStatusResponse } from '#lib/api.ts';
import { json, sentBody, stubFetch, type FetchCall } from '#lib/test/fetch.ts';
import AndroidTv from './AndroidTv.svelte';

const TITLE = () => m.android_tv_title();

function boxStatus(over: Partial<AndroidTvStatus> = {}, config: AndroidTvConfig = { host: '192.168.1.153' }): AndroidTvStatusResponse {
	return {
		success: true,
		config,
		status: { configured: true, reachable: true, awake: true, currentApp: 'org.smarttube.beta', model: 'LEAP-S1', paired: false, ...over }
	};
}

/** The box's backend: the status at GET /api/androidtv, `{ success }` elsewhere; records calls. */
function serve(status: AndroidTvStatusResponse | (() => Response), answers: Record<string, () => Response | Promise<Response>> = {}) {
	return stubFetch((url, init) => {
		const key = `${init?.method ?? 'GET'} ${url}`;
		if (answers[key]) return answers[key]();
		if (key === 'GET /api/androidtv') return typeof status === 'function' ? status() : json(status);
		return json({ success: true, message: 'ok' });
	});
}

const reads = (calls: FetchCall[]) => calls.filter((c) => c.url === '/api/androidtv' && (c.init?.method ?? 'GET') === 'GET').length;
const sent = (calls: FetchCall[]) => calls.filter((c) => c.init?.method && c.init.method !== 'GET').map((c) => [c.init!.method, c.url, sentBody(c)]);

async function activate(name: string) {
	const key = page.getByRole('button', { name, exact: true });
	await expect.element(key).toBeInTheDocument();
	key.element().dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }));
}

/** An upload the test drives by hand (installApk goes through XMLHttpRequest for its progress). */
const xhrs: FakeXhr[] = [];
class FakeXhr {
	upload: { onprogress?: (e: Partial<ProgressEvent>) => void; onload?: () => void } = {};
	onload?: () => void;
	onerror?: () => void;
	status = 0;
	response: unknown = null;
	withCredentials = false;
	responseType = '';
	url = '';
	method = '';
	body: FormData | null = null;
	constructor() {
		xhrs.push(this);
	}
	open(method: string, url: string) {
		this.method = method;
		this.url = url;
	}
	send(body: FormData) {
		this.body = body;
	}
}

beforeEach(() => {
	vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList);
});
afterEach(() => forgetAll());

describe('AndroidTv', () => {
	it('awake: a pressed toggle named after the box, the state and the current app', async () => {
		const calls = serve(boxStatus());
		await render(AndroidTv);
		await expect.element(page.getByRole('button', { name: TITLE(), exact: true })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(m.android_tv_awake(), { exact: true })).toBeVisible();
		const apps = page.getByRole('group', { name: m.android_tv_apps() });
		await expect.element(apps.getByRole('button', { name: 'SmartTube' })).toHaveAttribute('aria-current', 'true');
		await expect.element(apps.getByRole('button', { name: 'Iris' })).not.toHaveAttribute('aria-current');
		await expect.element(page.getByRole('group', { name: m.tv_pad({ name: TITLE() }) })).toBeVisible();
		await expect.element(page.getByText(m.android_tv_paired())).not.toBeInTheDocument();
		expect(calls.map((c) => c.url)).toEqual(['/api/androidtv']);
	});

	it('asleep: the toggle wakes it, then the status is read once again', async () => {
		const calls = serve(boxStatus({ awake: false, currentApp: undefined }));
		await render(AndroidTv);
		await expect.element(page.getByText(m.android_tv_asleep(), { exact: true })).toBeVisible();
		const toggle = page.getByRole('button', { name: TITLE(), exact: true });
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
		await toggle.click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/androidtv/wake', null]]);
	});

	it('awake: the toggle puts it to sleep, shown at once', async () => {
		let release!: () => void;
		const gate = new Promise<void>((r) => (release = r));
		const calls = serve(boxStatus(), {
			'GET /api/androidtv': async () => {
				if (calls.length > 1) await gate;
				return json(boxStatus());
			}
		});
		await render(AndroidTv);
		const toggle = page.getByRole('button', { name: TITLE(), exact: true });
		await toggle.click();
		await expect.poll(() => sent(calls)).toEqual([['POST', '/api/androidtv/sleep', null]]);
		// the answer patches the state before the re-read lands
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
		release();
	});

	it('an app shortcut launches it (waking the TV), says so and re-reads once', async () => {
		const toast = vi.spyOn(ui, 'toast');
		const calls = serve(boxStatus());
		await render(AndroidTv);
		await page.getByRole('button', { name: 'Iris' }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['POST', '/api/androidtv/launch', { package: 'studio.kahn.iris.tv', ensureTvOn: true }]]);
		expect(toast).toHaveBeenCalledWith(m.android_tv_launched({ name: 'Iris' }));
	});

	it('the configured favourites replace the default shortcuts', async () => {
		serve(boxStatus({ currentApp: 'com.netflix' }, { host: 'h', favouriteApps: [{ package: 'com.netflix', label: 'Netflix' }] }));
		await render(AndroidTv);
		await expect.element(page.getByRole('button', { name: 'Netflix' })).toHaveAttribute('aria-current', 'true');
		await expect.element(page.getByRole('button', { name: 'SmartTube' })).not.toBeInTheDocument();
	});

	it('every key goes out under its Android name, with no status read', async () => {
		const calls = serve(boxStatus());
		await render(AndroidTv);
		const names = [
			m.remote_keys_up(),
			m.tv_key_ok(),
			m.tv_key_back(),
			m.remote_keys_home(),
			m.remote_keys_menu(),
			m.remote_keys_search(),
			m.tv_key_previous(),
			m.tv_key_play_pause(),
			m.tv_key_next(),
			m.tv_volume_down(),
			m.tv_mute(),
			m.tv_volume_up()
		];
		for (const name of names) await activate(name);
		await expect.poll(() => sent(calls).length).toBe(names.length);
		expect(sent(calls).map(([, , b]) => b.key)).toEqual(['up', 'ok', 'back', 'home', 'menu', 'search', 'previous', 'play_pause', 'next', 'volume_down', 'mute', 'volume_up']);
		expect(sent(calls).every(([method, url]) => method === 'POST' && url === '/api/androidtv/key')).toBe(true);
		expect(reads(calls)).toBe(1);
	});

	it('unreachable: said in words, no toggle, keys and apps disabled', async () => {
		serve(boxStatus({ reachable: false, awake: false }));
		await render(AndroidTv);
		await expect.element(page.getByText(m.state_unreachable(), { exact: true })).toBeVisible();
		await expect.element(page.getByRole('button', { name: TITLE(), exact: true })).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: 'SmartTube' })).toBeDisabled();
		await expect.element(page.getByRole('button', { name: m.remote_keys_up() })).toBeDisabled();
		await expect.element(page.getByRole('button', { name: m.remote_keys_search() })).toBeDisabled();
	});

	it('not configured: says so, opens the settings, saving sends the address and re-reads once', async () => {
		const calls = serve(boxStatus({ configured: false, reachable: false, awake: false }, {}));
		await render(AndroidTv);
		await expect.element(page.getByText(m.android_tv_not_configured())).toBeVisible();
		await expect.element(page.getByText(m.android_tv_configure_hint())).toBeVisible();
		await expect.element(page.getByRole('group', { name: m.android_tv_apps() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.android_tv_pair() })).not.toBeInTheDocument();
		await userEvent.fill(page.getByLabelText(m.android_tv_host()).element(), '192.168.1.153');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([['PUT', '/api/androidtv/config', { host: '192.168.1.153' }]]);
	});

	it('paired: a chip says so, and the settings say the keys are fast', async () => {
		serve(boxStatus({ paired: true }));
		await render(AndroidTv);
		await expect.element(page.getByText(m.android_tv_paired(), { exact: true })).toBeVisible();
		const settings = page.getByRole('button', { name: m.common_settings() });
		await settings.click();
		await expect.element(settings).toHaveAttribute('aria-expanded', 'true');
		await expect.element(page.getByText(m.android_tv_paired_hint())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.android_tv_pair() })).not.toBeInTheDocument();
	});

	it('pairing: start, read the code off the TV, send it, then re-read once', async () => {
		const say = vi.spyOn(ui, 'say');
		const toast = vi.spyOn(ui, 'toast');
		const calls = serve(boxStatus());
		await render(AndroidTv);
		await page.getByRole('button', { name: m.common_settings() }).click();
		await page.getByRole('button', { name: m.android_tv_pair() }).click();
		const code = page.getByLabelText(m.android_tv_pair_code());
		await expect.element(code).toHaveFocus();
		expect(say).toHaveBeenCalledWith(m.android_tv_pair_started());
		const confirm = page.getByRole('button', { name: m.android_tv_pair_confirm() });
		await userEvent.fill(code.element(), 'AB12');
		await expect.element(confirm).toBeDisabled();
		await userEvent.fill(code.element(), 'AB12CD');
		await expect.element(confirm).toBeEnabled();
		await confirm.click();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(sent(calls)).toEqual([
			['POST', '/api/androidtv/pair/start', null],
			['POST', '/api/androidtv/pair/finish', { code: 'AB12CD' }]
		]);
		expect(toast).toHaveBeenCalledWith(m.android_tv_paired_ok());
		await expect.element(code).not.toBeInTheDocument();
	});

	it('an APK: the upload in percent, then « installing », then said installed', async () => {
		const toast = vi.spyOn(ui, 'toast');
		xhrs.length = 0;
		vi.stubGlobal('XMLHttpRequest', FakeXhr);
		const calls = serve(boxStatus());
		await render(AndroidTv);
		await page.getByRole('button', { name: m.common_settings() }).click();
		const input = page.getByLabelText(m.android_tv_install_apk());
		await userEvent.upload(input, new File(['apk'], 'iris.apk', { type: 'application/vnd.android.package-archive' }));
		await expect.poll(() => xhrs.length).toBe(1);
		const xhr = xhrs[0];
		expect([xhr.method, xhr.url, xhr.withCredentials]).toEqual(['POST', '/api/androidtv/apk', true]);
		expect((xhr.body?.get('apk') as File).name).toBe('iris.apk');

		xhr.upload.onprogress?.({ lengthComputable: true, loaded: 40, total: 100 });
		const bar = page.getByRole('progressbar', { name: m.android_tv_uploading({ name: 'iris.apk' }) });
		await expect.element(bar).toHaveAttribute('aria-valuetext', m.android_tv_upload_sent({ percent: 40 }));
		xhr.upload.onload?.();
		await expect.element(page.getByText(m.android_tv_installing())).toBeVisible();
		xhr.status = 200;
		xhr.response = { success: true, message: 'ok' };
		xhr.onload?.();
		await expect.poll(() => reads(calls)).toBe(2);
		expect(toast).toHaveBeenCalledWith(m.android_tv_apk_installed({ name: 'iris.apk' }));
		await expect.element(page.getByText(m.android_tv_installing())).not.toBeInTheDocument();
	});

	it('no answer at all: the tile says so', async () => {
		serve(() => json({ success: false, error: 'down' }, 502));
		await render(AndroidTv);
		await expect.element(page.getByText(m.command_no_answer_short())).toBeVisible();
	});
});
